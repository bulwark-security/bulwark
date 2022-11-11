use {
  crate::service::ExampleProcessor,
  clap::Parser,
  envoy_control_plane::envoy::service::ext_proc::v3::external_processor_server::ExternalProcessorServer,
  std::net::{IpAddr, Ipv4Addr, SocketAddr},
  tonic::transport::Server,
};

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
  let server = ExampleProcessor {};
  Server::builder()
    .add_service(ExternalProcessorServer::new(server))
    .serve(SocketAddr::new(
      IpAddr::V4(Ipv4Addr::UNSPECIFIED),
      args.port,
    ))
    .await?;
  Ok(())
}
