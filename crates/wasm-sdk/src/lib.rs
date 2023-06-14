//! This crate provides the logic for Bulwark's guest environment.

// Each macro invocation has to be scoped to its own mod to avoid fixed constant name collisions
#[allow(unused_macros)]
pub mod bulwark_host {
    wit_bindgen::generate!({
        world: "bulwark:plugin/host-calls"
    });
}
// Separate world for each handler because all handlers are optional
#[allow(unused_macros)]
pub mod request_handler {
    wit_bindgen::generate!({
        world: "bulwark:plugin/request-handler"
    });
}
#[allow(unused_macros)]
pub mod request_decision_handler {
    wit_bindgen::generate!({
        world: "bulwark:plugin/request-decision-handler"
    });
}
#[allow(unused_macros)]
pub mod response_decision_handler {
    wit_bindgen::generate!({
        world: "bulwark:plugin/response-decision-handler"
    });
}
#[allow(unused_macros)]
pub mod decision_feedback_handler {
    wit_bindgen::generate!({
        world: "bulwark:plugin/decision-feedback-handler"
    });
}

pub use bulwark_wasm_sdk_macros::handler;

mod errors;
mod from;
mod host_calls;

pub use bulwark_decision::*;
pub use errors::*;
pub use from::*;
pub use host_calls::*;

#[allow(unused_imports)]
#[macro_use]
extern crate approx;
