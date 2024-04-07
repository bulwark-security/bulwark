use bulwark_sdk::*;
use std::collections::HashMap;

pub struct BobPlugin;

#[bulwark_plugin]
impl HttpHandlers for BobPlugin {
    fn handle_request_decision(
        request: Request,
        _labels: HashMap<String, String>,
    ) -> Result<HandlerOutput, Error> {
        let mut output = HandlerOutput::default();
        if let Some(content_type) = request.headers().get("Content-Type") {
            if content_type.len() > 3 {
                output.decision = Decision::restricted(0.4);
                output.tags.push("no-reason".to_string());
            }
        }
        output
            .labels
            .insert("overwrite".to_string(), "new value".to_string());
        Ok(output)
    }

    fn handle_response_decision(
        _request: Request,
        _response: Response,
        _labels: HashMap<String, String>,
    ) -> Result<HandlerOutput, Error> {
        let mut output = HandlerOutput::default();
        output.tags.push("just-because".to_string());
        Ok(output)
    }

    fn handle_decision_feedback(
        _request: Request,
        _response: Response,
        _labels: HashMap<String, String>,
        _verdict: Verdict,
    ) -> Result<(), Error> {
        Ok(())
    }
}
