//! This crate provides the logic for Bulwark's guest environment.

pub use bulwark_wasm_sdk_macros::{bulwark_plugin, handler};

// Each macro invocation has to be scoped to its own mod to avoid fixed constant name collisions
#[allow(unused_macros)]
#[doc(hidden)]
pub mod bulwark_host {
    wit_bindgen::generate!({
        world: "bulwark:plugin/host-api"
    });
}
#[allow(unused_macros)]
#[doc(hidden)]
pub mod handlers {
    wit_bindgen::generate!({
        world: "bulwark:plugin/handlers"
    });
}

mod errors;
mod from;
mod host_api;

pub use bulwark_decision::*;
pub use errors::*;
pub use from::*;
/// The handler functions a plugin needs to expose to process requests and generate decisions.
///
/// See the [`bulwark_plugin`](https://docs.rs/bulwark-wasm-sdk/latest/bulwark_wasm_sdk/attr.bulwark_plugin.html)
/// attribute for additional details on how to use this trait.
pub use handlers::Handlers;
pub use host_api::*;
