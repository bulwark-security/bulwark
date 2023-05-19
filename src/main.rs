use axum::ServiceExt;

mod ecs;
mod errors;

use {
    axum::{extract::Path, extract::State, http::StatusCode, response::Json, routing::get, Router},
    bulwark_ext_processor::BulwarkProcessor,
    clap::{Parser, Subcommand},
    color_eyre::eyre::Result,
    envoy_control_plane::envoy::service::ext_proc::v3::external_processor_server::ExternalProcessorServer,
    errors::*,
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
struct Cli {
    /// Log levels: error, warn, info, debug, trace
    ///
    /// Default is "info".
    #[arg(short, long)]
    log_level: Option<String>,

    /// Log formats: ecs, forest
    ///
    /// Default is "ecs".
    #[arg(short, long)]
    log_format: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

/// The subcommands supported by the Bulwark CLI.
#[derive(Subcommand)]
enum Commands {
    /// Launch as an Envoy external processor
    ExtProcessor {
        /// Sets a custom config file
        #[arg(short, long, value_name = "FILE")]
        config: PathBuf,
    },
    // TODO: Implement ReverseProxy subcommand
    // TODO: Implement Check subcommand
    // TODO: Implement Test subcommand
    // TODO: Implement Compile subcommand
}

/// The health state structure tracks the health of the primary service, primarily for the benefit of
/// external monitoring by load balancers or container orchestration systems. It does not track a
/// liveness value because this will always be true if the process is running.
struct HealthState {
    /// Indicates that the primary service has successfully initialized and is ready to receive requests.
    /// Once true, it will remain true for the lifetime of the process.
    started: bool,
    /// Indicates that the primary service is ready to receive requests.
    /// In theory, this could become false after the service has started if the system detects that it's
    /// entered a deadlock state, however this is not currently implemented and does not meaningfully
    /// differ from the started state.
    ready: bool,
}

/// The health response structure determines the JSON serialization for health probe responses. Regardless
/// of the type of probe requested, all 3 health states will be reported for convenience. Generally,
/// automated systems requesting a health probe only consider the status code. This response body
/// benefits human operators while not excluding machine-readability.
#[derive(Serialize)]
struct HealthResponse {
    /// The live field is always true and indicates that the process running the primary service has
    /// started without immediate error but may not yet be ready to receive requests. The health status
    /// endpoints are not available until after configuration has been read, so if this endpoint can
    /// return a response at all, this value will be true.
    pub live: bool,
    /// The started field becomes true after the primary service is ready to receive requests. This occurs
    /// after all plugins have been compiled but before any plugin is instantiated by an incoming request.
    /// Once true, it will never become false.
    pub started: bool,
    /// The ready field indicates that the primary service is ready to receive requests. Theoretically,
    /// this could become false if a service becomes unlikely to ever be able serve requests again in the
    /// future, and should be set to false if the desired behavior is to trigger a restart. Currently the
    /// ready field has identical behavior to the started field since no detection for things like
    /// deadlock states exist yet.
    pub ready: bool,
}

/// The default probe handler is intended to be at the apex of the health check resource. It simply performs
/// a liveness health check by default.
///
/// See probe_handler.
async fn default_probe_handler(
    State(state): State<Arc<Mutex<HealthState>>>,
) -> (StatusCode, Json<HealthResponse>) {
    probe_handler(State(state), Path(String::from("live"))).await
}

/// The probe handler returns a JSON HealthResponse with a status code that depends on the probe type requested.
///
/// - live - Always returns an HTTP OK status if the endpoint is serving requests.
/// - started - Returns an HTTP OK status if the primary service has started and is ready to receive requests
///     and a Service Unavailable status otherwise.
/// - ready - Returns an HTTP OK status if the primary service is available and ready to receive requests
///     and a Service Unavailable status otherwise.
async fn probe_handler(
    State(state): State<Arc<Mutex<HealthState>>>,
    Path(probe): Path<String>,
) -> (StatusCode, Json<HealthResponse>) {
    let state = state.lock().unwrap();
    let status = match probe.as_str() {
        "live" => StatusCode::OK,
        "started" => {
            if state.started {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            }
        }
        "ready" => {
            if state.ready {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            }
        }
        // hint that the wrong probe value was sent
        _ => StatusCode::NOT_FOUND,
    };
    (
        status,
        Json(HealthResponse {
            live: true,
            started: state.started,
            ready: state.ready,
        }),
    )
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
    "debug,cranelift_codegen=info,wasmtime_cranelift=info,reqwest=info,hyper=info,h2=info";
/// An [`EnvFilter`] pattern to limit matched log events to trace events.
const TRACE_FILTER: &str = "trace";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TODO: tokio runtime builder to control runtime parameters

    let cli = Cli::parse();

    color_eyre::install()?;

    LogTracer::init().expect("log tracer init failed");

    let log_level = cli.log_level.unwrap_or_else(|| "info".to_string());
    let log_format = cli.log_format.unwrap_or_else(|| "ecs".to_string());
    let mut ecs_layer = None;
    let mut forest_layer = None;
    match log_format.as_str() {
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
                log_format,
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
                _ => log_level.as_str(),
            },
        ));
    tracing::subscriber::set_global_default(subscriber).unwrap();

    // You can check for the existence of subcommands, and if found use their
    // matches just as you would the top level cmd
    match &cli.command {
        Some(Commands::ExtProcessor { config }) => {
            let mut service_tasks: JoinSet<std::result::Result<(), ServiceError>> = JoinSet::new();

            let config_root = bulwark_config::toml::load_config(config)?;
            let port = config_root.service.port;
            let admin_port = config_root.service.admin_port;
            let admin_enabled = config_root.service.admin;
            let health_state = Arc::new(Mutex::new(HealthState {
                started: false,
                ready: false,
            }));

            // TODO: need a reference to the bulwark processor to pass to the admin service but that doesn't exist yet

            if admin_enabled {
                let health_state = health_state.clone();

                // TODO: make admin service optional
                service_tasks.spawn(async move {
                    // And run our service using `hyper`.
                    let addr = SocketAddr::from((IpAddr::V4(Ipv4Addr::UNSPECIFIED), admin_port));
                    let app = NormalizePathLayer::trim_trailing_slash().layer(
                        Router::new()
                            .route("/health", get(default_probe_handler)) // :probe is optional and defaults to liveness probe
                            .route("/health/:probe", get(probe_handler))
                            .with_state(health_state),
                    );

                    axum::Server::bind(&addr)
                        .serve(app.into_make_service())
                        .await
                        .map_err(ServiceError::AdminService)
                });
            }

            let bulwark_processor = BulwarkProcessor::new(config_root)?;
            let ext_filter = ExternalProcessorServer::new(bulwark_processor);

            {
                let health_state = health_state.clone();

                service_tasks.spawn(async move {
                    {
                        let mut health_state = health_state.lock().unwrap();
                        health_state.started = true;
                        health_state.ready = true;
                    }
                    Server::builder()
                        .add_service(ext_filter)
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
        None => todo!(),
    }

    // Continued program logic goes here...
    Ok(())
}
