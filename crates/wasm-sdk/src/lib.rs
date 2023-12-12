//! This crate provides the logic for Bulwark's guest environment.

pub use bulwark_wasm_sdk_macros::{bulwark_plugin, handler};

/// Internally used bindings.
#[doc(hidden)]
pub mod wit {
    wit_bindgen::generate!({
        world: "bulwark:plugin/platform",
    });
}

// Due to https://github.com/bytecodealliance/wit-bindgen/issues/674 we don't call `generate!` for
// the handlers and instead define the trait manually and do the bindings through our own macro.

mod errors;
mod from;
mod host_api;

pub use bulwark_decision::*;
pub use errors::*;
pub use from::*;
pub use host_api::*;
