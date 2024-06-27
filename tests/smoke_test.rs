use bulwark_host::{Plugin, PluginCtx, PluginInstance, RedisCtx, ScriptRegistry};
use bulwark_sdk::value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

#[tokio::test]
async fn test_smoke() -> Result<(), Box<dyn std::error::Error>> {
    let base = Path::new(file!()).parent().unwrap_or(Path::new("."));

    bulwark_build::build_plugin(
        base.join("plugins/smoke-test"),
        base.join("dist/plugins/smoke_test.wasm"),
        &[],
        true,
    )?;
    assert!(base.join("dist/plugins/smoke_test.wasm").exists());

    let redis_uri: String;
    if let Ok(redis_env) = std::env::var("REDIS_URI") {
        redis_uri = redis_env;
    } else {
        redis_uri = "redis://127.0.0.1:6379".to_string();
    }

    let config = &bulwark_config::Config {
        service: bulwark_config::Service {
            proxy_hops: 1,
            ..Default::default()
        },
        runtime: bulwark_config::Runtime::default(),
        state: bulwark_config::State {
            redis_uri: Some(redis_uri),
            ..Default::default()
        },
        thresholds: bulwark_config::Thresholds::default(),
        metrics: bulwark_config::Metrics::default(),
        secrets: vec![],
        plugins: vec![bulwark_config::Plugin {
            reference: "smoke_test".to_string(),
            location: bulwark_config::PluginLocation::Local(PathBuf::from(
                base.join("dist/plugins/smoke_test.wasm")
                    .to_str()
                    .unwrap()
                    .to_string(),
            )),
            access: bulwark_config::PluginAccess::None,
            verification: bulwark_config::PluginVerification::None,
            weight: 1.0,
            config: value!({
                "smoke": true,
                "complex": {
                    "color": "blue",
                    "size": 13
                }
            })
            .as_object()
            .expect("expected valid literal")
            .clone(),
            permissions: bulwark_config::Permissions {
                env: vec![],
                http: vec![
                    "gateway.docker.internal".to_string(),
                    "localhost".to_string(),
                    "127.0.0.1".to_string(),
                ],
                state: vec!["smoke".to_string(), "bulwark".to_string()],
            },
        }],
        presets: vec![],
        resources: vec![],
    };
    let plugin = Arc::new(Plugin::from_file(
        base.join("dist/plugins/smoke_test.wasm"),
        config,
        config.plugin("smoke_test").unwrap(),
    )?);
    let request = Arc::new(
        http::Request::builder()
            .method("GET")
            .uri("/")
            .header("X-Forwarded-For", "1.2.3.4")
            .version(http::Version::HTTP_11)
            .body(bytes::Bytes::new())?,
    );
    let redis_pool: Option<Arc<deadpool_redis::Pool>> =
        if let Some(redis_addr) = config.state.redis_uri.as_ref() {
            let cfg = deadpool_redis::Config {
                url: Some(redis_addr.into()),
                connection: None,
                pool: Some(deadpool_redis::PoolConfig::new(
                    config.state.redis_pool_size,
                )),
            };
            Some(Arc::new(
                cfg.create_pool(Some(deadpool_redis::Runtime::Tokio1))?,
            ))
        } else {
            None
        };
    let redis_ctx = RedisCtx {
        pool: redis_pool,
        registry: Arc::new(ScriptRegistry::default()),
    };
    let plugin_ctx = PluginCtx::new(plugin.clone(), HashMap::new(), redis_ctx)?;
    let mut plugin_instance = PluginInstance::new(plugin, plugin_ctx).await?;

    // Initialize the plugin.
    plugin_instance.handle_init().await?;

    // Perform request enrichment.
    let new_labels = plugin_instance
        .handle_request_enrichment(request.clone(), HashMap::new())
        .await?;
    assert!(!new_labels.is_empty());

    // Handle request decision.
    let handler_output = plugin_instance
        .handle_request_decision(request.clone(), new_labels.clone())
        .await?;

    assert_eq!(handler_output.decision.accept, 0.0);
    assert_eq!(handler_output.decision.restrict, 0.0);
    assert_eq!(handler_output.decision.unknown, 1.0);
    assert!(handler_output.tags.is_empty());

    let tags = handler_output.tags;
    let mut combined_labels = new_labels.clone();
    combined_labels.extend(handler_output.labels);

    let response = Arc::new(
        http::Response::builder()
            .status(200)
            .header("Content-Length", "11")
            .body(bytes::Bytes::from("hello world"))?,
    );

    // Handle response decision.
    let handler_output = plugin_instance
        .handle_response_decision(request.clone(), response.clone(), combined_labels.clone())
        .await?;

    assert_eq!(handler_output.decision.accept, 0.0);
    assert_eq!(handler_output.decision.restrict, 0.0);
    assert_eq!(handler_output.decision.unknown, 1.0);
    // This is now empty because this handler is a no-op. The host merges tags from all handlers.
    assert!(handler_output.tags.is_empty());

    let mut combined_labels = combined_labels.clone();
    combined_labels.extend(handler_output.labels);

    // Handle decision feedback (no output)
    plugin_instance
        .handle_decision_feedback(
            request.clone(),
            response.clone(),
            combined_labels,
            bulwark_sdk::Verdict {
                decision: handler_output.decision,
                outcome: bulwark_sdk::Outcome::Accepted,
                count: 1,
                tags: tags.iter().cloned().collect(),
            },
        )
        .await?;

    Ok(())
}
