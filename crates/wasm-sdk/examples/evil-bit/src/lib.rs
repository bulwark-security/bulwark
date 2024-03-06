use bulwark_wasm_sdk::*;
use std::collections::HashMap;

pub struct EvilBit;

#[bulwark_plugin]
impl HttpHandlers for EvilBit {
    /// Check to see if the request has confessed malicious intent by setting an `Evil` header.
    fn handle_request_decision(
        request: Request,
        _params: HashMap<String, String>,
    ) -> Result<HandlerOutput, Error> {
        let mut output = HandlerOutput::default();
        let evil_header = request.headers().get("Evil");
        if let Some(value) = evil_header {
            if value == "true" {
                output.decision = Decision {
                    accept: 0.0,
                    restrict: 1.0,
                    unknown: 0.0,
                };
                output.tags = vec!["evil".to_string()];
                return Ok(output);
            }
        }
        Ok(output)
    }
}
