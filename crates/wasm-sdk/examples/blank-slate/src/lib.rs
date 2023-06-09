use bulwark_wasm_sdk::*;

pub struct BlankSlate;

#[bulwark_plugin]
impl Handlers for BlankSlate {
    fn on_request() -> Result {
        // Cross-plugin communication logic goes here, or leave as a no-op.
        Ok(())
    }

    fn on_request_decision() -> Result {
        let _request = get_request();
        set_decision(Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 1.0,
        })?;
        set_tags(["blank-slate"]);
        Ok(())
    }

    fn on_response_decision() -> Result {
        // Process responses from the interior service, or leave as a no-op.
        let _request = get_request();
        let _response = get_response();
        Ok(())
    }

    fn on_decision_feedback() -> Result {
        // Feedback loop implementations go here, or leave as a no-op.
        Ok(())
    }
}
