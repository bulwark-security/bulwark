use bulwark_host::{Plugin, PluginCtx, PluginInstance, RedisCtx, ScriptRegistry};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

#[tokio::test]
async fn test_redis_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let base = Path::new(file!()).parent().unwrap_or(Path::new("."));

    bulwark_build::build_plugin(
        base.join("plugins/redis-plugin"),
        base.join("dist/plugins/redis_plugin.wasm"),
        &[],
        true,
    )?;
    assert!(base.join("dist/plugins/redis_plugin.wasm").exists());

    let redis_uri: String;
    if let Ok(redis_env) = std::env::var("REDIS_URI") {
        redis_uri = redis_env;
    } else {
        redis_uri = "redis://127.0.0.1:6379".to_string();
    }

    let config = &bulwark_config::Config {
        service: bulwark_config::Service::default(),
        runtime: bulwark_config::Runtime::default(),
        state: bulwark_config::State {
            redis_uri: Some(redis_uri),
            ..Default::default()
        },
        thresholds: bulwark_config::Thresholds::default(),
        metrics: bulwark_config::Metrics::default(),
        secrets: vec![],
        plugins: vec![bulwark_config::Plugin {
            reference: "redis_plugin".to_string(),
            location: bulwark_config::PluginLocation::Local(PathBuf::from(
                base.join("dist/plugins/redis_plugin.wasm")
                    .to_str()
                    .unwrap()
                    .to_string(),
            )),
            access: bulwark_config::PluginAccess::None,
            verification: bulwark_config::PluginVerification::None,
            weight: 1.0,
            config: serde_json::map::Map::new(),
            permissions: bulwark_config::Permissions {
                env: vec![],
                http: vec![],
                state: vec!["test".to_string(), "bulwark".to_string()],
            },
        }],
        presets: vec![],
        resources: vec![],
    };
    let plugin = Arc::new(Plugin::from_file(
        base.join("dist/plugins/redis_plugin.wasm"),
        config,
        config.plugin("redis_plugin").unwrap(),
    )?);
    let request = Arc::new(
        http::Request::builder()
            .method("GET")
            .uri("/")
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
    assert!(new_labels.is_empty());

    // Handle request decision.
    let handler_output = plugin_instance
        .handle_request_decision(request.clone(), new_labels)
        .await?;

    assert_eq!(handler_output.decision.accept, 0.0);
    assert_eq!(handler_output.decision.restrict, 0.0);
    assert_eq!(handler_output.decision.unknown, 1.0);
    assert!(handler_output.tags.is_empty());

    let tags = handler_output.tags;

    let response = Arc::new(
        http::Response::builder()
            .status(200)
            .body(bytes::Bytes::new())?,
    );

    // Handle response decision.
    let handler_output = plugin_instance
        .handle_response_decision(request.clone(), response.clone(), handler_output.labels)
        .await?;

    assert_eq!(handler_output.decision.accept, 0.0);
    assert_eq!(handler_output.decision.restrict, 0.0);
    assert_eq!(handler_output.decision.unknown, 1.0);
    // This is now empty because this handler is a no-op. The host merges tags from all handlers.
    assert!(handler_output.tags.is_empty());

    // Handle decision feedback (no output)
    plugin_instance
        .handle_decision_feedback(
            request.clone(),
            response.clone(),
            handler_output.labels,
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
