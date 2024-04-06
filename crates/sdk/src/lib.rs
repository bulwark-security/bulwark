//! This crate provides the logic for Bulwark's guest environment.

pub use bulwark_sdk_macros::{bulwark_plugin, handler};

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
pub mod redis;

pub use bulwark_decision::*;
pub use errors::*;
pub use host_api::*;

/// Make wit_bindgen accessible to the macro crate, ensuring that the correct version is used.
#[doc(hidden)]
pub use wit_bindgen;
