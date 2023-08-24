mod admin;
mod build;
mod ecs;
mod errors;

use {
    crate::admin::{AdminState, HealthState, MetricsState},
    axum::{
        extract::Path, extract::State, http::StatusCode, response::Json, routing::get, Router,
        ServiceExt,
    },
    bulwark_ext_processor::BulwarkProcessor,
    clap::{Parser, Subcommand},
    color_eyre::eyre::Result,
    envoy_control_plane::envoy::service::ext_proc::v3::external_processor_server::ExternalProcessorServer,
    errors::*,
    metrics_exporter_prometheus::Matcher,
    metrics_exporter_statsd::StatsdBuilder,
    serde::Serialize,
    std::net::{IpAddr, Ipv4Addr, SocketAddr},
    std::{
        path::PathBuf,
        sync::{Arc, Mutex},
    },
    tokio::task::JoinSet,
    tonic::transport::Server,
    tower_http::normalize_path::NormalizePathLayer,
    tower_layer::Layer,
    tracing::error,
    tracing_forest::ForestLayer,
    tracing_log::LogTracer,
    tracing_subscriber::layer::SubscriberExt,
    tracing_subscriber::{EnvFilter, Registry},
};

/// bulwark-cli launches and interacts with the Bulwark service.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[clap(arg_required_else_help = true)]
struct Cli {
    /// Log levels: error, warn, info, debug, trace
    ///
    /// Default is "info".
    #[arg(short = 'l', long)]
    log_level: Option<String>,

    /// Log formats: ecs, forest
    ///
    /// Default is "ecs".
    #[arg(short = 'f', long)]
    log_format: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

/// The subcommands supported by the Bulwark CLI.
#[derive(Subcommand)]
enum Command {
    /// Launch as an Envoy external processor
    ExtProcessor {
        /// Sets a custom config file
        #[arg(short, long, value_name = "FILE")]
        config: PathBuf,
    },
    // TODO: Implement ReverseProxy subcommand
    // TODO: Implement Check subcommand
    // TODO: Implement Test subcommand
    /// Compile a Bulwark plugin
    Build {
        /// Sets the input directory for the build.
        ///
        /// Default is the current working directory.
        #[arg(short, long, value_name = "FILE")]
        path: Option<PathBuf>,
        /// Sets the output file for the build.
        ///
        /// Default is `./dist/name_of_plugin.wasm`.
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,

        /// Additional arguments passed through to the compiler.
        #[arg(last = true)]
        compiler_args: Vec<String>,
    },
}

/// An [`EnvFilter`] pattern to limit matched log events to error events.
const ERROR_FILTER: &str = "error";
/// An [`EnvFilter`] pattern to limit matched log events to warning events.
const WARN_FILTER: &str = "warn";
/// An [`EnvFilter`] pattern to limit matched log events to informational events.
const INFO_FILTER: &str = "info";
/// An [`EnvFilter`] pattern to limit matched log events to debug events.
///
/// The debug filter is more selective due to libraries increasing log verbosity too quickly.
/// It filters out several libraries that would otherwise have low relevance in debug logs.
/// The unfiltered behavior can still be accessed by specifying `debug,` with a trailing comma
/// as the log level argument.
const DEBUG_FILTER: &str =
    "debug,cranelift_codegen=info,wasmtime_cranelift=info,wasmtime_jit=info,reqwest=info,hyper=info,h2=info";
/// An [`EnvFilter`] pattern to limit matched log events to trace events.
const TRACE_FILTER: &str = "trace";

fn init_tracing(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    color_eyre::install()?;

    LogTracer::init().expect("log tracer init failed");

    let log_level: &str = cli.log_level.as_ref().map_or("info", |ll| ll.as_str());
    let log_format: &str = cli.log_format.as_ref().map_or("ecs", |lf| lf.as_str());
    let mut ecs_layer = None;
    let mut forest_layer = None;
    match log_format {
        "ecs" => {
            ecs_layer = Some(ForestLayer::from(
                tracing_forest::Printer::new().formatter(crate::ecs::EcsFormatter),
            ));
            forest_layer = None;
        }
        "forest" => {
            ecs_layer = None;
            forest_layer = Some(ForestLayer::default());
        }
        _ => {
            Err(crate::errors::CliArgumentError::InvalidLogFormat(
                log_format.to_string(),
            ))?;
        }
    }

    let subscriber = Registry::default()
        .with(ecs_layer)
        .with(forest_layer)
        // TODO: refine filter to hide extraneous info from libraries
        // TODO: behavior should be that library events are visible only at the TRACE level
        .with(EnvFilter::new(
            match log_level.to_ascii_lowercase().as_str() {
                "error" => ERROR_FILTER,
                "warn" => WARN_FILTER,
                "info" => INFO_FILTER,
                "debug" => DEBUG_FILTER,
                "trace" => TRACE_FILTER,
                _ => log_level,
            },
        ));
    tracing::subscriber::set_global_default(subscriber).unwrap();
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TODO: tokio runtime builder to control runtime parameters

    let cli = Cli::parse();
    init_tracing(&cli)?;

    // You can check for the existence of subcommands, and if found use their
    // matches just as you would the top level cmd
    match &cli.command.ok_or(CliArgumentError::MissingSubcommand)? {
        Command::ExtProcessor { config } => {
            let mut service_tasks: JoinSet<std::result::Result<(), ServiceError>> = JoinSet::new();

            let config_root = bulwark_config::toml::load_config(config)?;
            let port = config_root.service.port;
            let admin_port = config_root.service.admin_port;
            let admin_enabled = config_root.service.admin_enabled;
            let prometheus_handle;

            if let Some(statsd_host) = &config_root.metrics.statsd_host {
                prometheus_handle = None;
                let prefix = if config_root.metrics.statsd_prefix.as_str().is_empty() {
                    None
                } else {
                    Some(config_root.metrics.statsd_prefix.as_str())
                };
                let recorder = StatsdBuilder::from(
                    statsd_host,
                    config_root.metrics.statsd_port.unwrap_or(8125),
                )
                .with_queue_size(config_root.metrics.statsd_queue_size)
                .with_buffer_size(config_root.metrics.statsd_buffer_size)
                .histogram_is_distribution()
                .build(prefix)
                .map_err(MetricsError::from)?;

                metrics::set_boxed_recorder(Box::new(recorder)).map_err(MetricsError::from)?;
            } else {
                let thresholds = config_root.thresholds;
                prometheus_handle = Some(
                    crate::admin::PrometheusBuilder::new()
                        // Setting buckets forces histograms to be rendered as native histograms rather than summaries
                        .set_buckets_for_metric(
                            Matcher::Suffix("decision_score".to_string()),
                            &[
                                thresholds.trust,
                                thresholds.suspicious,
                                thresholds.restrict,
                                1.0,
                            ],
                        )
                        .map_err(MetricsError::from)?
                        .set_buckets_for_metric(
                            Matcher::Suffix("decision_conflict".to_string()),
                            &[0.01, 0.1, 0.25, 0.5, 0.75, 1.0, 5.0],
                        )
                        .map_err(MetricsError::from)?
                        .set_buckets_for_metric(
                            Matcher::Full("combined_conflict".to_string()),
                            &[0.01, 0.1, 0.25, 0.5, 0.75, 1.0, 5.0],
                        )
                        .map_err(MetricsError::from)?
                        .install_recorder()
                        .map_err(MetricsError::from)?,
                );

                // TODO: Enable process metrics collection. (libproc.h issue, maybe behind cfg feature)
                // let process = metrics_process::Collector::default();
                // process.describe();
            }

            let admin_state = Arc::new(Mutex::new(AdminState {
                health: HealthState {
                    live: true,
                    started: false,
                    ready: false,
                },
                metrics: MetricsState::new(
                    prometheus_handle,
                    // TODO: Enable process metrics collection. (libproc.h issue, maybe behind cfg feature)
                    // collect: move || process.collect(),
                ),
            }));

            // TODO: need a reference to the bulwark processor to pass to the admin service but that doesn't exist yet

            if admin_enabled {
                let admin_state = admin_state.clone();

                service_tasks.spawn(async move {
                    // And run our service using `hyper`.
                    let addr = SocketAddr::from((IpAddr::V4(Ipv4Addr::UNSPECIFIED), admin_port));
                    let app = NormalizePathLayer::trim_trailing_slash().layer(
                        Router::new()
                            .route("/health", get(admin::default_probe_handler)) // :probe is optional and defaults to liveness probe
                            .route("/health/:probe", get(admin::probe_handler))
                            .route("/metrics", get(admin::metrics_handler))
                            .with_state(admin_state),
                    );

                    axum::Server::bind(&addr)
                        .serve(app.into_make_service())
                        .await
                        .map_err(ServiceError::AdminService)
                });
            }

            let bulwark_processor = BulwarkProcessor::new(config_root)?;
            let ext_processor = ExternalProcessorServer::new(bulwark_processor);

            {
                let admin_state = admin_state.clone();

                service_tasks.spawn(async move {
                    {
                        let mut admin_state = admin_state.lock().expect("poisoned mutex");
                        admin_state.health.started = true;
                        admin_state.health.ready = true;
                    }
                    Server::builder()
                        .add_service(ext_processor)
                        .serve(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port)) // TODO: make socket addr configurable?
                        .await
                        .map_err(ServiceError::ExtProcessorService)
                });
            }

            while let Some(r) = service_tasks.join_next().await {
                match r {
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => error!(
                        message = "service could not start",
                        error_message = ?e,
                    ),
                    Err(e) => error!(
                        message = "join error on service initialization",
                        error_message = ?e,
                    ),
                }
            }
        }
        Command::Build {
            path,
            output,
            compiler_args,
        } => {
            let current_dir = std::env::current_dir()?;
            let path = path.clone().unwrap_or(current_dir.clone());
            let wasm_filename = crate::build::wasm_filename(&path)?;
            // Defaults to joining with the current working directory, not the input path
            let mut output = output
                .clone()
                .unwrap_or(current_dir.join("dist/plugins").join(&wasm_filename));
            if output.is_dir() {
                output = output.join(wasm_filename);
            }
            crate::build::build_plugin(&path, output, compiler_args)?;
        }
    }

    Ok(())
}
