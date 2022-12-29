use bulwark_ext_filter::BulwarkProcessor;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use {
    envoy_control_plane::envoy::service::ext_proc::v3::external_processor_server::ExternalProcessorServer,
    std::net::{IpAddr, Ipv4Addr, SocketAddr},
    tonic::transport::Server,
};

use color_eyre::eyre::Result;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_forest::ForestLayer;
use tracing_log::LogTracer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TODO: tokio runtime builder to control runtime parameters

    let cli = Cli::parse();

    color_eyre::install()?;

    LogTracer::init().expect("log tracer init failed");

    let subscriber = Registry::default()
        .with(ForestLayer::default())
        .with(EnvFilter::new("WARN"))
        .with(JsonStorageLayer);
    tracing::subscriber::set_global_default(subscriber).unwrap();

    // You can check for the existence of subcommands, and if found use their
    // matches just as you would the top level cmd
    match &cli.command {
        Some(Commands::ExtFilter { config }) => {
            let config_root = bulwark_config::load_config(config)?;
            let port = config_root.port();
            let server = BulwarkProcessor::new(config_root)?;
            Server::builder()
                .add_service(ExternalProcessorServer::new(server))
                .serve(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port))
                .await?;
        }
        None => todo!(),
    }

    // Continued program logic goes here...
    Ok(())
}
