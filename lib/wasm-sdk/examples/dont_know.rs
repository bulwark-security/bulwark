use bulwark_wasm_sdk::traits::*;
use bulwark_wasm_sdk::types::*;

bulwark_wasm_sdk::main! {{
    bulwark_wasm_sdk::set_root_context(|_| -> Box<dyn RootContext> { Box::new(DontKnowRoot) });
}}

struct DontKnowRoot;

impl Context for DontKnowRoot {}

impl RootContext for DontKnowRoot {
    fn get_type(&self) -> Option<ContextType> {
        Some(ContextType::HttpContext)
    }

    fn create_http_context(&self, _: u32) -> Option<Box<dyn HttpContext>> {
        Some(Box::new(DontKnow {}))
    }
}

struct DontKnow {}

impl Context for DontKnow {}

impl HttpContext for DontKnow {
    fn on_http_request_decision(&mut self, _: usize, _: usize, _: bool) -> Decision {
        Decision {
            accept: 0.0,
            restrict: 0.0,
            // This detection serves no purpose beyond minimalism. It provides no information to the decision.
            unknown: 1.0,
        }
    }
}
