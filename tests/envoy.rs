use bulwark_ext_processor::BulwarkProcessor;
use envoy_control_plane::envoy::service::ext_proc::v3::external_processor_server::ExternalProcessorServer;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use tokio::task::JoinSet;
use tonic::transport::Server;

// Ignored by default because it requires a running envoy instance with a specific configuration.
// See ./github/workflows/rust.yml for more information.
// Use local_envoy.yaml config for local testing. The gha_envoy.yaml config is used by GitHub Actions.
#[tokio::test]
#[ignore]
async fn test_envoy_evil_bit() -> Result<(), Box<dyn std::error::Error>> {
    let base = Path::new(file!()).parent().unwrap_or(Path::new("."));

    bulwark_build::build_plugin(
        base.join("../crates/sdk/examples/evil-bit"),
        base.join("dist/plugins/bulwark_evil_bit.wasm"),
        &[],
        true,
    )?;
    assert!(base.join("dist/plugins/bulwark_evil_bit.wasm").exists());

    let mut tasks: JoinSet<std::result::Result<(), anyhow::Error>> = JoinSet::new();

    let config_root = bulwark_config::toml::load_config(&base.join("bulwark.toml"))?;
    let port = config_root.service.port;
    let bulwark_processor = BulwarkProcessor::new(config_root).await?;
    let ext_processor = ExternalProcessorServer::new(bulwark_processor);

    {
        tasks.spawn(async move {
            Server::builder()
                .add_service(ext_processor)
                .serve(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port))
                .await
                .map_err(anyhow::Error::from)
        });
    }

    // wait for the server to finish starting before sending requests
    tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

    // send a friendly request to our envoy service
    let response = reqwest::get("http://127.0.0.1:8080").await?;
    assert!(response.status().is_success());
    let body = response.text().await?;
    assert!(body.contains("hello-world"));

    // send an evil request to our envoy service
    let client = reqwest::Client::new();
    let response = client
        .get("http://127.0.0.1:8080")
        .header("Evil", "true")
        .send()
        .await?;
    assert!(response.status().is_client_error());
    let body = response.text().await?;
    assert!(body.contains("Access Denied"));

    Ok(())
}

#[tokio::test]
#[ignore]
async fn test_envoy_multi_phase_exec() -> Result<(), Box<dyn std::error::Error>> {
    let base = Path::new(file!()).parent().unwrap_or(Path::new("."));

    bulwark_build::build_plugin(
        base.join("plugins/multi-phase-plugin-a"),
        base.join("dist/plugins/multi_phase_plugin_a.wasm"),
        &[],
        true,
    )?;
    assert!(base.join("dist/plugins/multi_phase_plugin_a.wasm").exists());
    bulwark_build::build_plugin(
        base.join("plugins/multi-phase-plugin-b"),
        base.join("dist/plugins/multi_phase_plugin_b.wasm"),
        &[],
        true,
    )?;
    assert!(base.join("dist/plugins/multi_phase_plugin_b.wasm").exists());

    let mut tasks: JoinSet<std::result::Result<(), anyhow::Error>> = JoinSet::new();

    let config_root = bulwark_config::toml::load_config(&base.join("multi_phase.toml"))?;
    let port = config_root.service.port;
    let bulwark_processor = BulwarkProcessor::new(config_root).await?;
    let ext_processor = ExternalProcessorServer::new(bulwark_processor);

    {
        tasks.spawn(async move {
            Server::builder()
                .add_service(ext_processor)
                .serve(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port))
                .await
                .map_err(anyhow::Error::from)
        });
    }

    // wait for the server to finish starting before sending requests
    tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

    // send a POST request to our envoy service
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:8080")
        .header("Content-Type", "text/html")
        .send()
        .await?;
    println!("resp: {response:?}");
    assert!(response.status().is_client_error());
    let body = response.text().await?;
    assert!(body.contains("Access Denied"));

    // send a GET request to our envoy service
    let client = reqwest::Client::new();
    let response = client
        .get("http://127.0.0.1:8080")
        .header("Content-Type", "text/html")
        .send()
        .await?;
    assert!(response.status().is_success());
    let body = response.text().await?;
    assert!(body.contains("hello-world"));

    Ok(())
}
