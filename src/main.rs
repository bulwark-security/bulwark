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

struct AdminState {
    started: bool,
    ready: bool,
}

#[derive(Serialize)]
struct HealthResponse {
    pub live: bool,
    pub started: bool,
    pub ready: bool,
}

async fn admin_handler(State(state): State<Arc<Mutex<AdminState>>>) -> Json<HealthResponse> {
    let state = state.lock().unwrap();
    Json(HealthResponse {
        live: true,
        started: state.started,
        ready: state.ready,
    })
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
            let admin_state = Arc::new(Mutex::new(AdminState {
                started: false,
                ready: false,
            }));

            // TODO: need a reference to the bulwark processor to pass to the admin service but that doesn't exist yet

            if admin_enabled {
                let admin_service = ServiceBuilder::new().service_fn(admin_handler);
                let admin_state = admin_state.clone();

                // TODO: make admin service optional
                service_tasks.spawn(async move {
                    // And run our service using `hyper`.
                    let addr = SocketAddr::from((IpAddr::V4(Ipv4Addr::UNSPECIFIED), admin_port));
                    let app = Router::new()
                        .route("/", get(admin_handler))
                        .with_state(admin_state);

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
                let admin_state = admin_state.clone();

                service_tasks.spawn(async move {
                    {
                        let mut admin_state = admin_state.lock().unwrap();
                        admin_state.started = true;
                        admin_state.ready = true;
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
