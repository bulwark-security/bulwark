use bulwark_wasm_sdk::traits::*;
use bulwark_wasm_sdk::types::*;
use log::trace;

bulwark_wasm_sdk::main! {{
    bulwark_wasm_sdk::set_log_level(LogLevel::Trace);
    bulwark_wasm_sdk::set_root_context(|_| -> Box<dyn RootContext> { Box::new(EvilParamRoot) });
}}

struct EvilParamRoot;

impl Context for EvilParamRoot {}

impl RootContext for EvilParamRoot {
    fn get_type(&self) -> Option<ContextType> {
        Some(ContextType::HttpContext)
    }

    fn create_http_context(&self, context_id: u32) -> Option<Box<dyn HttpContext>> {
        Some(Box::new(EvilParam { context_id }))
    }
}

struct EvilParam {
    context_id: u32,
}

impl Context for EvilParam {}

impl HttpContext for EvilParam {
    fn on_http_request_decision(&mut self, _: usize, _: usize, _: bool) -> Decision {
        for (name, value) in &self.get_http_request_headers() {
            trace!("#{} -> {}: {}", self.context_id, name, value);
        }

        match self.get_http_request_header(":query") {
            Some(query) if query.contains("evil=true") => Decision {
                accept: 0.0,
                restrict: 1.0,
                unknown: 0.0,
            },
            _ => Decision {
                accept: 0.0,
                restrict: 0.0,
                // See RFC 3514 for discussion on why we can draw no conclusion
                // from the absence of this important parameter.
                unknown: 1.0,
            },
        }
    }

    fn on_log(&mut self) {
        trace!("#{} completed.", self.context_id);
    }
}
