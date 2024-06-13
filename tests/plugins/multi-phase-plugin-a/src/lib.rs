use bulwark_sdk::*;
use std::collections::HashMap;

pub struct AlicePlugin;

#[bulwark_plugin]
impl HttpHandlers for AlicePlugin {
    fn handle_request_enrichment(
        _request: http::Request,
        labels: HashMap<String, String>,
    ) -> Result<HashMap<String, String>, Error> {
        let mut new_labels = HashMap::new();
        new_labels.insert("enriched".to_string(), "true".to_string());
        new_labels.insert("overwrite".to_string(), "original value".to_string());
        if labels.contains_key("userid") {
            new_labels.insert("userid".to_string(), "bob".to_string());
        }
        Ok(new_labels)
    }

    fn handle_request_decision(
        request: http::Request,
        _labels: HashMap<String, String>,
    ) -> Result<HandlerOutput, Error> {
        let mut output = HandlerOutput::default();
        if request.method() == "GET" {
            output.decision = Decision::restricted(0.0);
        } else {
            output.decision = Decision::restricted(0.333);
        }
        output.tags.push("always-set".to_string());
        Ok(output)
    }

    fn handle_response_decision(
        request: http::Request,
        response: http::Response,
        labels: HashMap<String, String>,
    ) -> Result<HandlerOutput, Error> {
        let mut output = HandlerOutput::default();
        // Include some test cases directly in the plugin to handle Envoy end-to-end testing.
        if let Some(overwrite) = labels.get("overwrite") {
            assert_eq!(overwrite.as_str(), "new value");
        }
        if response.status() == http::StatusCode::OK && request.method() == "POST" {
            // This needs to be low enough that it doesn't trigger a block on its own, but
            // high enough that it triggers a block only when combined with another restrict
            // value that originated in a _request_ phase in a different plugin that similarly
            // isn't enough to block on its own. This has to be triggered by the Envoy
            // integration test to hit the behavior this is intended to verify.
            output.decision = Decision::restricted(0.5);
        } else if response.status() == http::StatusCode::NOT_FOUND {
            if let Some(overwrite) = labels.get("overwrite") {
                if overwrite.as_str() == "new value" {
                    output.decision = Decision::restricted(0.1);
                }
            }
        } else if response.status() == http::StatusCode::INTERNAL_SERVER_ERROR {
            return Err(error!(
                "this is an intentional error to force the error tag"
            ));
        }
        Ok(output)
    }
}
