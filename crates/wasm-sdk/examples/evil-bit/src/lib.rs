use bulwark_wasm_sdk::*;

pub struct EvilBit;

#[bulwark_plugin]
impl Handlers for EvilBit {
    /// Check to see if the request has confessed malicious intent by setting an `Evil` header.
    fn on_request_decision() -> Result {
        let request = get_request();
        let evil_header = request.headers().get("Evil");
        if let Some(value) = evil_header {
            if value == "true" {
                set_decision(Decision {
                    accept: 0.0,
                    restrict: 1.0,
                    unknown: 0.0,
                })?;
                set_tags(["evil"]);
                return Ok(());
            }
        }
        set_decision(Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 1.0,
        })?;
        Ok(())
    }
}
