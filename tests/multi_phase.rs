use approx::assert_relative_eq;
use bulwark_host::{Plugin, PluginCtx, PluginInstance, RedisCtx, ScriptRegistry};
use bulwark_sdk::Decision;
use std::{collections::HashMap, collections::HashSet, path::Path, sync::Arc};

#[test]
fn test_multi_phase_exec_post_request() -> Result<(), Box<dyn std::error::Error>> {
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

    let host_config = bulwark_config::toml::load_config(&base.join("multi_phase.toml"))?;
    let plugin_a = Arc::new(Plugin::from_file(
        base.join("dist/plugins/multi_phase_plugin_a.wasm"),
        // None of this config will get read during this test.
        &host_config,
        &host_config.plugins[0],
    )?);
    let plugin_b = Arc::new(Plugin::from_file(
        base.join("dist/plugins/multi_phase_plugin_b.wasm"),
        // None of this config will get read during this test.
        &host_config,
        &host_config.plugins[1],
    )?);
    let request = Arc::new(
        http::Request::builder()
            .method("POST")
            .uri("/example")
            .version(http::Version::HTTP_11)
            .header("Content-Type", "application/json")
            .body(bytes::Bytes::new())?,
    );
    let redis_ctx = RedisCtx {
        pool: None,
        registry: Arc::new(ScriptRegistry::default()),
    };
    let router_labels = HashMap::new();
    let plugin_ctx_a = PluginCtx::new(plugin_a.clone(), HashMap::default(), redis_ctx.clone())?;
    let mut plugin_instance_a = tokio_test::block_on(PluginInstance::new(plugin_a, plugin_ctx_a))?;
    let plugin_ctx_b = PluginCtx::new(plugin_b.clone(), HashMap::default(), redis_ctx.clone())?;
    let mut plugin_instance_b = tokio_test::block_on(PluginInstance::new(plugin_b, plugin_ctx_b))?;

    // Initialize the plugin.
    tokio_test::block_on(plugin_instance_a.handle_init())?;
    tokio_test::block_on(plugin_instance_b.handle_init())?;

    // Perform request enrichment.
    let new_labels_a = tokio_test::block_on(
        plugin_instance_a.handle_request_enrichment(request.clone(), router_labels.clone()),
    )?;
    let new_labels_b = tokio_test::block_on(
        plugin_instance_b.handle_request_enrichment(request.clone(), router_labels.clone()),
    )?;
    assert_eq!(
        new_labels_a,
        HashMap::from([
            ("enriched".to_string(), "true".to_string()),
            ("overwrite".to_string(), "original value".to_string())
        ])
    );
    assert_eq!(new_labels_b, HashMap::new());

    let mut combined_labels = router_labels.clone();
    combined_labels.extend(new_labels_a);
    combined_labels.extend(new_labels_b);

    assert_eq!(
        combined_labels,
        HashMap::from([
            ("enriched".to_string(), "true".to_string()),
            ("overwrite".to_string(), "original value".to_string())
        ])
    );

    // Handle request decision.
    let req_handler_output_a = tokio_test::block_on(
        plugin_instance_a.handle_request_decision(request.clone(), combined_labels.clone()),
    )?;
    let req_handler_output_b = tokio_test::block_on(
        plugin_instance_b.handle_request_decision(request.clone(), combined_labels.clone()),
    )?;

    assert_relative_eq!(
        req_handler_output_a.decision,
        Decision {
            accept: 0.0,
            restrict: 0.333,
            unknown: 0.667
        }
    );
    assert_eq!(
        req_handler_output_a.tags,
        HashSet::from(["always-set".to_string()])
    );
    assert_eq!(req_handler_output_a.labels, HashMap::new());
    assert_relative_eq!(
        req_handler_output_b.decision,
        Decision {
            accept: 0.0,
            restrict: 0.4,
            unknown: 0.6
        }
    );
    assert_eq!(
        req_handler_output_b.tags,
        HashSet::from(["no-reason".to_string()])
    );
    assert_eq!(
        req_handler_output_b.labels,
        HashMap::from([("overwrite".to_string(), "new value".to_string()),])
    );

    combined_labels.extend(req_handler_output_a.labels.clone());
    combined_labels.extend(req_handler_output_b.labels.clone());
    assert_eq!(
        combined_labels,
        HashMap::from([
            ("enriched".to_string(), "true".to_string()),
            ("overwrite".to_string(), "new value".to_string()),
        ])
    );

    let mut combined_tags: Vec<String> = Vec::new();
    combined_tags.extend(req_handler_output_a.tags.clone());
    combined_tags.extend(req_handler_output_b.tags.clone());
    assert_eq!(
        combined_tags,
        vec!["always-set".to_string(), "no-reason".to_string()]
    );

    let decision_vec: Vec<Decision> = [req_handler_output_a.clone(), req_handler_output_b.clone()]
        .iter()
        .map(|output| output.decision)
        .collect();
    let req_combined_decision = Decision::combine_murphy(&decision_vec);
    assert_relative_eq!(
        req_combined_decision,
        Decision {
            accept: 0.0,
            restrict: 0.599,
            unknown: 0.401
        },
        epsilon = 0.001
    );

    // Simple 404 response.
    let response = Arc::new(
        http::Response::builder()
            .status(404)
            .body(bytes::Bytes::from_static(b"Not Found"))?,
    );

    // Handle response decision.
    let resp_handler_output_a = tokio_test::block_on(plugin_instance_a.handle_response_decision(
        request.clone(),
        response.clone(),
        combined_labels.clone(),
    ))?;
    let resp_handler_output_b = tokio_test::block_on(plugin_instance_b.handle_response_decision(
        request.clone(),
        response.clone(),
        combined_labels.clone(),
    ))?;

    assert_relative_eq!(
        resp_handler_output_a.decision,
        Decision {
            accept: 0.0,
            restrict: 0.1,
            unknown: 0.9
        }
    );
    assert_eq!(resp_handler_output_a.tags, HashSet::new());
    assert_eq!(resp_handler_output_a.labels, HashMap::new());
    assert_relative_eq!(resp_handler_output_b.decision, Decision::default());
    assert_eq!(
        resp_handler_output_b.tags,
        HashSet::from(["just-because".to_string()])
    );
    assert_eq!(resp_handler_output_b.labels, HashMap::new());

    combined_labels.extend(resp_handler_output_a.labels.clone());
    combined_labels.extend(resp_handler_output_b.labels.clone());
    assert_eq!(
        combined_labels,
        HashMap::from([
            ("enriched".to_string(), "true".to_string()),
            ("overwrite".to_string(), "new value".to_string()),
        ])
    );

    combined_tags.extend(resp_handler_output_a.tags.clone());
    combined_tags.extend(resp_handler_output_b.tags.clone());
    assert_eq!(
        combined_tags,
        vec![
            "always-set".to_string(),
            "no-reason".to_string(),
            "just-because".to_string()
        ]
    );

    // Simulate the combining logic done.
    let decision_vec: Vec<Decision> = [
        if resp_handler_output_a.decision.is_unknown() {
            req_handler_output_a
        } else {
            resp_handler_output_a
        },
        if resp_handler_output_b.decision.is_unknown() {
            req_handler_output_b
        } else {
            resp_handler_output_b
        },
    ]
    .iter()
    .map(|output| output.decision)
    .collect();
    let resp_combined_decision = Decision::combine_murphy(&decision_vec);
    assert_relative_eq!(
        resp_combined_decision,
        Decision {
            accept: 0.0,
            restrict: 0.438,
            unknown: 0.562
        },
        epsilon = 0.001
    );

    let outcome = resp_combined_decision.outcome(0.2, 0.6, 0.8)?;
    assert_eq!(outcome, bulwark_sdk::Outcome::Suspected);

    // Handle decision feedback (no output)
    tokio_test::block_on(plugin_instance_a.handle_decision_feedback(
        request.clone(),
        response.clone(),
        combined_labels.clone(),
        bulwark_sdk::Verdict {
            decision: resp_combined_decision,
            outcome,
            count: 2,
            tags: combined_tags.clone(),
        },
    ))?;

    Ok(())
}

#[test]
fn test_multi_phase_exec_get_request() -> Result<(), Box<dyn std::error::Error>> {
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

    let host_config = bulwark_config::toml::load_config(&base.join("multi_phase.toml"))?;
    let plugin_a = Arc::new(Plugin::from_file(
        base.join("dist/plugins/multi_phase_plugin_a.wasm"),
        // None of this config will get read during this test.
        &host_config,
        &host_config.plugins[0],
    )?);
    let plugin_b = Arc::new(Plugin::from_file(
        base.join("dist/plugins/multi_phase_plugin_b.wasm"),
        // None of this config will get read during this test.
        &host_config,
        &host_config.plugins[1],
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
    let router_labels = HashMap::new();
    let plugin_ctx_a = PluginCtx::new(plugin_a.clone(), HashMap::default(), redis_ctx.clone())?;
    let mut plugin_instance_a = tokio_test::block_on(PluginInstance::new(plugin_a, plugin_ctx_a))?;
    let plugin_ctx_b = PluginCtx::new(plugin_b.clone(), HashMap::default(), redis_ctx.clone())?;
    let mut plugin_instance_b = tokio_test::block_on(PluginInstance::new(plugin_b, plugin_ctx_b))?;

    // Initialize the plugin.
    tokio_test::block_on(plugin_instance_a.handle_init())?;
    tokio_test::block_on(plugin_instance_b.handle_init())?;

    // Perform request enrichment.
    let new_labels_a = tokio_test::block_on(
        plugin_instance_a.handle_request_enrichment(request.clone(), router_labels.clone()),
    )?;
    let new_labels_b = tokio_test::block_on(
        plugin_instance_b.handle_request_enrichment(request.clone(), router_labels.clone()),
    )?;
    assert_eq!(
        new_labels_a,
        HashMap::from([
            ("enriched".to_string(), "true".to_string()),
            ("overwrite".to_string(), "original value".to_string())
        ])
    );
    assert_eq!(new_labels_b, HashMap::new());

    let mut combined_labels = router_labels.clone();
    combined_labels.extend(new_labels_a);
    combined_labels.extend(new_labels_b);

    assert_eq!(
        combined_labels,
        HashMap::from([
            ("enriched".to_string(), "true".to_string()),
            ("overwrite".to_string(), "original value".to_string())
        ])
    );

    // Handle request decision.
    let req_handler_output_a = tokio_test::block_on(
        plugin_instance_a.handle_request_decision(request.clone(), combined_labels.clone()),
    )?;
    let req_handler_output_b = tokio_test::block_on(
        plugin_instance_b.handle_request_decision(request.clone(), combined_labels.clone()),
    )?;

    assert_relative_eq!(req_handler_output_a.decision, Decision::default());
    assert_eq!(
        req_handler_output_a.tags,
        HashSet::from(["always-set".to_string()])
    );
    assert_eq!(req_handler_output_a.labels, HashMap::new());
    assert_relative_eq!(req_handler_output_b.decision, Decision::default());
    assert_eq!(req_handler_output_b.tags, HashSet::new());
    assert_eq!(
        req_handler_output_b.labels,
        HashMap::from([("overwrite".to_string(), "new value".to_string()),])
    );

    combined_labels.extend(req_handler_output_a.labels.clone());
    combined_labels.extend(req_handler_output_b.labels.clone());
    assert_eq!(
        combined_labels,
        HashMap::from([
            ("enriched".to_string(), "true".to_string()),
            ("overwrite".to_string(), "new value".to_string()),
        ])
    );

    let mut combined_tags: Vec<String> = Vec::new();
    combined_tags.extend(req_handler_output_a.tags.clone());
    combined_tags.extend(req_handler_output_b.tags.clone());
    assert_eq!(combined_tags, vec!["always-set".to_string()]);

    let decision_vec: Vec<Decision> = [req_handler_output_a.clone(), req_handler_output_b.clone()]
        .iter()
        .map(|output| output.decision)
        .collect();
    let req_combined_decision = Decision::combine_murphy(&decision_vec);
    assert_relative_eq!(req_combined_decision, Decision::default());

    // Simple 200 response.
    let response = Arc::new(
        http::Response::builder()
            .status(200)
            .body(bytes::Bytes::from_static(b"hello-world"))?,
    );

    // Handle response decision.
    let resp_handler_output_a = tokio_test::block_on(plugin_instance_a.handle_response_decision(
        request.clone(),
        response.clone(),
        combined_labels.clone(),
    ))?;
    let resp_handler_output_b = tokio_test::block_on(plugin_instance_b.handle_response_decision(
        request.clone(),
        response.clone(),
        combined_labels.clone(),
    ))?;

    assert_relative_eq!(resp_handler_output_a.decision, Decision::default());
    assert_eq!(resp_handler_output_a.tags, HashSet::new());
    assert_eq!(resp_handler_output_a.labels, HashMap::new());
    assert_relative_eq!(resp_handler_output_b.decision, Decision::default());
    assert_eq!(
        resp_handler_output_b.tags,
        HashSet::from(["just-because".to_string()])
    );
    assert_eq!(resp_handler_output_b.labels, HashMap::new());

    combined_labels.extend(resp_handler_output_a.labels.clone());
    combined_labels.extend(resp_handler_output_b.labels.clone());
    assert_eq!(
        combined_labels,
        HashMap::from([
            ("enriched".to_string(), "true".to_string()),
            ("overwrite".to_string(), "new value".to_string()),
        ])
    );

    combined_tags.extend(resp_handler_output_a.tags.clone());
    combined_tags.extend(resp_handler_output_b.tags.clone());
    assert_eq!(
        combined_tags,
        vec!["always-set".to_string(), "just-because".to_string()]
    );

    // Simulate the combining logic done.
    let decision_vec: Vec<Decision> = [
        if resp_handler_output_a.decision.is_unknown() {
            req_handler_output_a
        } else {
            resp_handler_output_a
        },
        if resp_handler_output_b.decision.is_unknown() {
            req_handler_output_b
        } else {
            resp_handler_output_b
        },
    ]
    .iter()
    .map(|output| output.decision)
    .collect();
    let resp_combined_decision = Decision::combine_murphy(&decision_vec);
    assert_relative_eq!(resp_combined_decision, Decision::default());

    let outcome = resp_combined_decision.outcome(0.2, 0.6, 0.8)?;
    assert_eq!(outcome, bulwark_sdk::Outcome::Accepted);

    // Handle decision feedback (no output)
    tokio_test::block_on(plugin_instance_a.handle_decision_feedback(
        request.clone(),
        response.clone(),
        combined_labels.clone(),
        bulwark_sdk::Verdict {
            decision: resp_combined_decision,
            outcome,
            count: 2,
            tags: combined_tags.clone(),
        },
    ))?;

    Ok(())
}

#[test]
fn test_multi_phase_exec_route_labels() -> Result<(), Box<dyn std::error::Error>> {
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

    let host_config = bulwark_config::toml::load_config(&base.join("multi_phase.toml"))?;
    let plugin_a = Arc::new(Plugin::from_file(
        base.join("dist/plugins/multi_phase_plugin_a.wasm"),
        // None of this config will get read during this test.
        &host_config,
        &host_config.plugins[0],
    )?);
    let plugin_b = Arc::new(Plugin::from_file(
        base.join("dist/plugins/multi_phase_plugin_b.wasm"),
        // None of this config will get read during this test.
        &host_config,
        &host_config.plugins[1],
    )?);
    let request = Arc::new(
        http::Request::builder()
            .method("OPTIONS")
            .uri("/user/alice")
            .version(http::Version::HTTP_11)
            .body(bytes::Bytes::new())?,
    );
    let redis_ctx = RedisCtx {
        pool: None,
        registry: Arc::new(ScriptRegistry::default()),
    };
    let router_labels = HashMap::from([("userid".to_string(), "alice".to_string())]);
    let plugin_ctx_a = PluginCtx::new(plugin_a.clone(), HashMap::default(), redis_ctx.clone())?;
    let mut plugin_instance_a = tokio_test::block_on(PluginInstance::new(plugin_a, plugin_ctx_a))?;
    let plugin_ctx_b = PluginCtx::new(plugin_b.clone(), HashMap::default(), redis_ctx.clone())?;
    let mut plugin_instance_b = tokio_test::block_on(PluginInstance::new(plugin_b, plugin_ctx_b))?;

    // Initialize the plugin.
    tokio_test::block_on(plugin_instance_a.handle_init())?;
    tokio_test::block_on(plugin_instance_b.handle_init())?;

    // Perform request enrichment.
    let new_labels_a = tokio_test::block_on(
        plugin_instance_a.handle_request_enrichment(request.clone(), router_labels.clone()),
    )?;
    let new_labels_b = tokio_test::block_on(
        plugin_instance_b.handle_request_enrichment(request.clone(), router_labels.clone()),
    )?;
    assert_eq!(
        new_labels_a,
        HashMap::from([
            ("userid".to_string(), "bob".to_string()),
            ("enriched".to_string(), "true".to_string()),
            ("overwrite".to_string(), "original value".to_string())
        ])
    );
    assert_eq!(new_labels_b, HashMap::new());

    let mut combined_labels = router_labels.clone();
    combined_labels.extend(new_labels_a);
    combined_labels.extend(new_labels_b);

    assert_eq!(
        combined_labels,
        HashMap::from([
            ("userid".to_string(), "bob".to_string()),
            ("enriched".to_string(), "true".to_string()),
            ("overwrite".to_string(), "original value".to_string())
        ])
    );

    // Handle request decision.
    let req_handler_output_a = tokio_test::block_on(
        plugin_instance_a.handle_request_decision(request.clone(), combined_labels.clone()),
    )?;
    let req_handler_output_b = tokio_test::block_on(
        plugin_instance_b.handle_request_decision(request.clone(), combined_labels.clone()),
    )?;

    assert_relative_eq!(
        req_handler_output_a.decision,
        Decision {
            accept: 0.0,
            restrict: 0.333,
            unknown: 0.667
        },
        epsilon = 0.001
    );
    assert_eq!(
        req_handler_output_a.tags,
        HashSet::from(["always-set".to_string()])
    );
    assert_eq!(req_handler_output_a.labels, HashMap::new());
    assert_relative_eq!(req_handler_output_b.decision, Decision::default());
    assert_eq!(req_handler_output_b.tags, HashSet::new());
    assert_eq!(
        req_handler_output_b.labels,
        HashMap::from([("overwrite".to_string(), "new value".to_string()),])
    );

    combined_labels.extend(req_handler_output_a.labels.clone());
    combined_labels.extend(req_handler_output_b.labels.clone());
    assert_eq!(
        combined_labels,
        HashMap::from([
            ("userid".to_string(), "bob".to_string()),
            ("enriched".to_string(), "true".to_string()),
            ("overwrite".to_string(), "new value".to_string()),
        ])
    );

    let mut combined_tags: Vec<String> = Vec::new();
    combined_tags.extend(req_handler_output_a.tags.clone());
    combined_tags.extend(req_handler_output_b.tags.clone());
    assert_eq!(combined_tags, vec!["always-set".to_string()]);

    let decision_vec: Vec<Decision> = [req_handler_output_a.clone(), req_handler_output_b.clone()]
        .iter()
        .map(|output| output.decision)
        .collect();
    let req_combined_decision = Decision::combine_murphy(&decision_vec);
    assert_relative_eq!(
        req_combined_decision,
        Decision {
            accept: 0.0,
            restrict: 0.305,
            unknown: 0.695
        },
        epsilon = 0.001
    );

    // Simple 204 response.
    let response = Arc::new(
        http::Response::builder()
            .status(204)
            .header("Allow", "OPTIONS, GET, HEAD, POST")
            .body(bytes::Bytes::new())?,
    );

    // Handle response decision.
    let resp_handler_output_a = tokio_test::block_on(plugin_instance_a.handle_response_decision(
        request.clone(),
        response.clone(),
        combined_labels.clone(),
    ))?;
    let resp_handler_output_b = tokio_test::block_on(plugin_instance_b.handle_response_decision(
        request.clone(),
        response.clone(),
        combined_labels.clone(),
    ))?;

    assert_relative_eq!(resp_handler_output_a.decision, Decision::default());
    assert_eq!(resp_handler_output_a.tags, HashSet::new());
    assert_eq!(resp_handler_output_a.labels, HashMap::new());
    assert_relative_eq!(resp_handler_output_b.decision, Decision::default());
    assert_eq!(
        resp_handler_output_b.tags,
        HashSet::from(["just-because".to_string()])
    );
    assert_eq!(resp_handler_output_b.labels, HashMap::new());

    combined_labels.extend(resp_handler_output_a.labels.clone());
    combined_labels.extend(resp_handler_output_b.labels.clone());
    assert_eq!(
        combined_labels,
        HashMap::from([
            ("userid".to_string(), "bob".to_string()),
            ("enriched".to_string(), "true".to_string()),
            ("overwrite".to_string(), "new value".to_string()),
        ])
    );

    combined_tags.extend(resp_handler_output_a.tags.clone());
    combined_tags.extend(resp_handler_output_b.tags.clone());
    assert_eq!(
        combined_tags,
        vec!["always-set".to_string(), "just-because".to_string()]
    );

    // Simulate the combining logic done.
    let decision_vec: Vec<Decision> = [
        if resp_handler_output_a.decision.is_unknown() {
            req_handler_output_a
        } else {
            resp_handler_output_a
        },
        if resp_handler_output_b.decision.is_unknown() {
            req_handler_output_b
        } else {
            resp_handler_output_b
        },
    ]
    .iter()
    .map(|output| output.decision)
    .collect();
    let resp_combined_decision = Decision::combine_murphy(&decision_vec);
    assert_relative_eq!(
        resp_combined_decision,
        Decision {
            accept: 0.0,
            restrict: 0.305,
            unknown: 0.695
        },
        epsilon = 0.001
    );

    let outcome = resp_combined_decision.outcome(0.2, 0.6, 0.8)?;
    assert_eq!(outcome, bulwark_sdk::Outcome::Suspected);

    // Handle decision feedback (no output)
    tokio_test::block_on(plugin_instance_a.handle_decision_feedback(
        request.clone(),
        response.clone(),
        combined_labels.clone(),
        bulwark_sdk::Verdict {
            decision: resp_combined_decision,
            outcome,
            count: 2,
            tags: combined_tags.clone(),
        },
    ))?;

    Ok(())
}
