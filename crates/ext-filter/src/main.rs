use bulwark_wasm_host::Plugin;

use {
    crate::service::BulwarkProcessor,
    clap::Parser,
    envoy_control_plane::envoy::service::ext_proc::v3::external_processor_server::ExternalProcessorServer,
    std::net::{IpAddr, Ipv4Addr, SocketAddr},
    tonic::transport::Server,
};

mod errors;
mod service;

#[derive(Parser, Debug)]
#[clap()]
struct Args {
    #[clap(short, long)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    println!("Server listening on port {}", args.port);
    let wasm_bytes = include_bytes!("../../wasm-host/test/bulwark-evil-bit.wasm");
    let plugin = std::sync::Arc::new(Plugin::from_bytes(wasm_bytes)?);
    let server = BulwarkProcessor { plugin };
    Server::builder()
        .add_service(ExternalProcessorServer::new(server))
        .serve(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            args.port,
        ))
        .await?;
    Ok(())
}
