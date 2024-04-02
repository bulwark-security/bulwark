use bulwark_wasm_host::{Plugin, PluginCtx, PluginInstance, RedisCtx, ScriptRegistry};
use std::{collections::HashMap, path::Path, sync::Arc};

#[test]
fn test_blank_slate_exec() -> Result<(), Box<dyn std::error::Error>> {
    let base = Path::new(file!()).parent().unwrap_or(Path::new("."));

    bulwark_build::build_plugin(
        base.join("../crates/wasm-sdk/examples/blank-slate"),
        base.join("dist/plugins/bulwark_blank_slate.wasm"),
        &[],
        true,
    )?;
    assert!(base.join("dist/plugins/bulwark_blank_slate.wasm").exists());

    let plugin = Arc::new(Plugin::from_file(
        base.join("dist/plugins/bulwark_blank_slate.wasm"),
        // None of this config will get read during this test.
        &bulwark_config::Config {
            service: bulwark_config::Service::default(),
            runtime: bulwark_config::Runtime::default(),
            state: bulwark_config::State::default(),
            thresholds: bulwark_config::Thresholds::default(),
            metrics: bulwark_config::Metrics::default(),
            plugins: vec![],
            presets: vec![],
            resources: vec![],
        },
        &bulwark_config::Plugin::default(),
    )?);
    let request = Arc::new(
        http::Request::builder()
            .method("GET")
            .uri("/")
            .version(http::Version::HTTP_11)
            .body(bytes::Bytes::new())?,
    );
    let redis_ctx = RedisCtx {
        pool: None,
        registry: Arc::new(ScriptRegistry::default()),
    };
    let plugin_ctx = PluginCtx::new(plugin.clone(), HashMap::new(), redis_ctx)?;
    let mut plugin_instance = tokio_test::block_on(PluginInstance::new(plugin, plugin_ctx))?;

    // Initialize the plugin.
    tokio_test::block_on(plugin_instance.handle_init())?;

    // Perform request enrichment.
    let new_labels = tokio_test::block_on(
        plugin_instance.handle_request_enrichment(request.clone(), HashMap::new()),
    )?;
    assert!(new_labels.is_empty());

    // Handle request decision.
    let handler_output =
        tokio_test::block_on(plugin_instance.handle_request_decision(request.clone(), new_labels))?;

    assert_eq!(handler_output.decision.accept, 0.0);
    assert_eq!(handler_output.decision.restrict, 0.0);
    assert_eq!(handler_output.decision.unknown, 1.0);
    assert!(handler_output.tags.contains("blank-slate"));

    let tags = handler_output.tags;

    let response = Arc::new(
        http::Response::builder()
            .status(200)
            .body(bytes::Bytes::new())?,
    );

    // Handle response decision.
    let handler_output = tokio_test::block_on(plugin_instance.handle_response_decision(
        request.clone(),
        response.clone(),
        handler_output.labels,
    ))?;

    assert_eq!(handler_output.decision.accept, 0.0);
    assert_eq!(handler_output.decision.restrict, 0.0);
    assert_eq!(handler_output.decision.unknown, 1.0);
    // This is now empty because this handler is a no-op. The host merges tags from all handlers.
    assert!(handler_output.tags.is_empty());

    // Handle decision feedback (no output)
    tokio_test::block_on(plugin_instance.handle_decision_feedback(
        request.clone(),
        response.clone(),
        handler_output.labels,
        bulwark_wasm_sdk::Verdict {
            decision: handler_output.decision,
            outcome: bulwark_wasm_sdk::Outcome::Accepted,
            tags: tags.iter().cloned().collect(),
        },
    ))?;

    Ok(())
}

#[test]
fn test_evil_bit_benign_exec() -> Result<(), Box<dyn std::error::Error>> {
    let base = Path::new(file!()).parent().unwrap_or(Path::new("."));

    bulwark_build::build_plugin(
        base.join("../crates/wasm-sdk/examples/evil-bit"),
        base.join("dist/plugins/bulwark_evil_bit.wasm"),
        &[],
        true,
    )?;
    assert!(base.join("dist/plugins/bulwark_evil_bit.wasm").exists());

    let plugin = Arc::new(Plugin::from_file(
        base.join("dist/plugins/bulwark_evil_bit.wasm"),
        // None of this config will get read during this test.
        &bulwark_config::Config {
            service: bulwark_config::Service::default(),
            runtime: bulwark_config::Runtime::default(),
            state: bulwark_config::State::default(),
            thresholds: bulwark_config::Thresholds::default(),
            metrics: bulwark_config::Metrics::default(),
            plugins: vec![],
            presets: vec![],
            resources: vec![],
        },
        &bulwark_config::Plugin::default(),
    )?);
    let request = Arc::new(
        http::Request::builder()
            .method("GET")
            .uri("/")
            .version(http::Version::HTTP_11)
            .body(bytes::Bytes::new())?,
    );
    let redis_ctx = RedisCtx {
        pool: None,
        registry: Arc::new(ScriptRegistry::default()),
    };
    let plugin_ctx = PluginCtx::new(plugin.clone(), HashMap::new(), redis_ctx)?;
    let mut plugin_instance = tokio_test::block_on(PluginInstance::new(plugin, plugin_ctx))?;

    // Initialize the plugin.
    tokio_test::block_on(plugin_instance.handle_init())?;

    // Perform request enrichment.
    let new_labels = tokio_test::block_on(
        plugin_instance.handle_request_enrichment(request.clone(), HashMap::new()),
    )?;
    assert!(new_labels.is_empty());

    // Handle request decision.
    let handler_output =
        tokio_test::block_on(plugin_instance.handle_request_decision(request.clone(), new_labels))?;

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
    let handler_output = tokio_test::block_on(plugin_instance.handle_response_decision(
        request.clone(),
        response.clone(),
        handler_output.labels,
    ))?;

    assert_eq!(handler_output.decision.accept, 0.0);
    assert_eq!(handler_output.decision.restrict, 0.0);
    assert_eq!(handler_output.decision.unknown, 1.0);
    assert!(handler_output.tags.is_empty());

    // Handle decision feedback (no output)
    tokio_test::block_on(plugin_instance.handle_decision_feedback(
        request.clone(),
        response.clone(),
        handler_output.labels,
        bulwark_wasm_sdk::Verdict {
            decision: handler_output.decision,
            outcome: bulwark_wasm_sdk::Outcome::Accepted,
            tags: tags.iter().cloned().collect(),
        },
    ))?;

    Ok(())
}

#[test]
fn test_evil_bit_evil_exec() -> Result<(), Box<dyn std::error::Error>> {
    let base = Path::new(file!()).parent().unwrap_or(Path::new("."));

    bulwark_build::build_plugin(
        base.join("../crates/wasm-sdk/examples/evil-bit"),
        base.join("dist/plugins/evil-bit.wasm"),
        &[],
        true,
    )?;
    assert!(base.join("dist/plugins/evil-bit.wasm").exists());

    let plugin = Arc::new(Plugin::from_file(
        base.join("dist/plugins/evil-bit.wasm"),
        // None of this config will get read during this test.
        &bulwark_config::Config {
            service: bulwark_config::Service::default(),
            runtime: bulwark_config::Runtime::default(),
            state: bulwark_config::State::default(),
            thresholds: bulwark_config::Thresholds::default(),
            metrics: bulwark_config::Metrics::default(),
            plugins: vec![],
            presets: vec![],
            resources: vec![],
        },
        &bulwark_config::Plugin::default(),
    )?);
    let request = Arc::new(
        http::Request::builder()
            .method("POST")
            .uri("/example")
            .version(http::Version::HTTP_11)
            .header("Content-Type", "application/json")
            .header("Evil", "true")
            .body(bytes::Bytes::new())?,
    );
    let redis_ctx = RedisCtx {
        pool: None,
        registry: Arc::new(ScriptRegistry::default()),
    };
    let plugin_ctx = PluginCtx::new(plugin.clone(), HashMap::new(), redis_ctx)?;
    let mut plugin_instance = tokio_test::block_on(PluginInstance::new(plugin, plugin_ctx))?;

    // Initialize the plugin.
    tokio_test::block_on(plugin_instance.handle_init())?;

    // Perform request enrichment.
    let new_labels = tokio_test::block_on(
        plugin_instance.handle_request_enrichment(request.clone(), HashMap::new()),
    )?;
    assert!(new_labels.is_empty());

    // Handle request decision.
    let handler_output =
        tokio_test::block_on(plugin_instance.handle_request_decision(request.clone(), new_labels))?;

    assert_eq!(handler_output.decision.accept, 0.0);
    assert_eq!(handler_output.decision.restrict, 1.0);
    assert_eq!(handler_output.decision.unknown, 0.0);
    assert!(handler_output.tags.contains("evil"));

    let tags = handler_output.tags;

    // No response decision because we'd block.
    let response = Arc::new(
        http::Response::builder()
            .status(403)
            .body(bytes::Bytes::new())?,
    );

    // Handle decision feedback (no output)
    tokio_test::block_on(plugin_instance.handle_decision_feedback(
        request.clone(),
        response.clone(),
        handler_output.labels,
        bulwark_wasm_sdk::Verdict {
            decision: handler_output.decision,
            outcome: bulwark_wasm_sdk::Outcome::Restricted,
            tags: tags.iter().cloned().collect(),
        },
    ))?;

    Ok(())
}
