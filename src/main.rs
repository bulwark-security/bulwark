use bulwark_ext_filter::BulwarkProcessor;
use clap::{Parser, Subcommand};
use std::{path::PathBuf, sync::Arc};
use tokio::task::JoinSet;

use {
    envoy_control_plane::envoy::service::ext_proc::v3::external_processor_server::ExternalProcessorServer,
    std::net::{IpAddr, Ipv4Addr, SocketAddr},
    tonic::transport::Server,
};

use color_eyre::eyre::Result;
use http::{Request, Response};
use hyper::{Body, Error};
use tower::{make::Shared, ServiceBuilder};
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, instrument, trace, warn, Instrument};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_forest::ForestLayer;
use tracing_log::LogTracer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

mod errors;

use errors::*;

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

async fn admin_handler(request: Request<Body>) -> Result<Response<Body>, Error> {
    Ok(Response::new(Body::from("{\"live\":true}\n")))
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

            let bulwark_processor = BulwarkProcessor::new(config_root)?;
            let ext_filter = ExternalProcessorServer::new(bulwark_processor);

            service_tasks.spawn(async move {
                Server::builder()
                    .add_service(ext_filter.clone()) // ExternalProcessorServer is clonable but BulwarkProcessor is not
                    .serve(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port)) // TODO: make socket addr configurable?
                    .await
                    .map_err(ServiceError::ExtFilterServiceError)
            });

            if admin_enabled {
                let admin_service = ServiceBuilder::new()
                    .layer(TraceLayer::new_for_http())
                    .service_fn(admin_handler);

                // TODO: make admin service optional
                service_tasks.spawn(async move {
                    // And run our service using `hyper`.
                    let addr = SocketAddr::from((IpAddr::V4(Ipv4Addr::UNSPECIFIED), admin_port));
                    hyper::server::Server::bind(&addr)
                        .serve(Shared::new(admin_service))
                        .await
                        .map_err(ServiceError::AdminServiceError)
                });
            }

            tokio::task::yield_now().await;

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
