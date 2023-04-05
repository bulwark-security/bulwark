use axum::extract::Path;
use serde_json::json;

mod errors;

use {
    axum::{extract::State, http::StatusCode, response::Json, routing::get, Router},
    bulwark_ext_filter::BulwarkProcessor,
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
    tower::ServiceBuilder,
    tracing::{debug, error, info, instrument, trace, warn, Instrument},
    tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer},
    tracing_forest::ForestLayer,
    tracing_log::LogTracer,
    tracing_subscriber::layer::SubscriberExt,
    tracing_subscriber::{EnvFilter, Registry},
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Log levels: [trace][debug][info][warning|warn][error][critical][off]
    ///
    /// Default is [info]
    #[arg(short, long)]
    log_level: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Launch as an Envoy external filter
    ExtFilter {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TODO: tokio runtime builder to control runtime parameters

    let cli = Cli::parse();

    color_eyre::install()?;

    LogTracer::init().expect("log tracer init failed");

    let subscriber = Registry::default()
        .with(ForestLayer::default())
        .with(EnvFilter::new(
            cli.log_level.unwrap_or_else(|| "INFO".to_string()),
        ))
        .with(JsonStorageLayer);
    tracing::subscriber::set_global_default(subscriber).unwrap();

    // You can check for the existence of subcommands, and if found use their
    // matches just as you would the top level cmd
    match &cli.command {
        Some(Commands::ExtFilter { config }) => {
            let mut service_tasks: JoinSet<std::result::Result<(), ServiceError>> = JoinSet::new();

            let config_root = bulwark_config::load_config(config)?;
            // TODO: just have serde defaults, this is kinda gross
            let port = config_root.port();
            let admin_port = config_root.admin_port();
            let admin_enabled = config_root.admin_service_enabled();
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
                    let app = Router::new()
                        .route("/", get(default_probe_handler)) // :probe is optional and defaults to liveness probe
                        .route("/:probe", get(probe_handler))
                        .with_state(health_state);

                    axum::Server::bind(&addr)
                        .serve(app.into_make_service())
                        .await
                        .map_err(ServiceError::AdminServiceError)
                });
            }

            tokio::task::yield_now().await;

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
                        .add_service(ext_filter.clone()) // ExternalProcessorServer is clonable but BulwarkProcessor is not
                        .serve(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port)) // TODO: make socket addr configurable?
                        .await
                        .map_err(ServiceError::ExtFilterServiceError)
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
