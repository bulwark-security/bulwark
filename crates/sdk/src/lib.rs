//! # SDK for Bulwark plugins
//!
//! The Bulwark SDK provides an API for writing detection plugins. Plugins are compiled to WebAssembly components
//! and executed by [Bulwark](https://bulwark.security/) in a secure sandboxed environment.
//!
//! ## Writing A Plugin
//!
//! A typical Bulwark detection plugin will declare a struct that implements the generated
//! `HttpHandlers` trait. This trait is provided by the [`bulwark_plugin`] macro.
//!
//! ## Minimal Example
//!
#![cfg_attr(doctest, doc = "````no_test")] // highlight, but don't run the test (rust/issues/63193)
//! ```rust
//! use bulwark_sdk::*;
//! use std::collections::HashMap;
//!
//! pub struct ExamplePlugin;
//!
//! #[bulwark_plugin]
//! impl HttpHandlers for ExamplePlugin {
//!     fn handle_request_decision(
//!         _request: http::Request,
//!         _labels: HashMap<String, String>,
//!     ) -> Result<HandlerOutput, Error> {
//!         let mut output = HandlerOutput::default();
//!         // Main detection logic goes here.
//!         output.decision = Decision::restricted(0.0);
//!         Ok(output)
//!     }
//! }
//! ```
//!
//! ## Building A Plugin
//!
//! Bulwark plugins are compiled to WebAssembly components. They can be compiled using the
//! `bulwark-cli build` subcommand. Detailed instructions are available in the
//! [main Bulwark documentation](https://bulwark.security/docs/guides/builds/building-plugins/).
//!
//! ## Handler Functions
//!
//! There are five handler functions that can be implemented by a plugin:
//!
//! - `handle_init`
//!     - Initializes a plugin. Rarely implemented.
//! - `handle_request_enrichment`
//!     - Enriches a request with labels prior to a decision being made. Can be used e.g. to decrypt session cookies.
//! - `handle_request_decision`
//!     - Makes a decision on a request before sending it to the interior service. Most plugins will implement this.
//! - `handle_response_decision`
//!     - Makes a decision on a response received from the interior service. Can be used e.g. for circuit breaking.
//! - `handle_decision_feedback`
//!     - Receives the combined decision after a [`Verdict`] has been rendered. Can be used e.g. for counters or
//!         unsupervised learning.
//!
//! Each handler function is called by the Bulwark runtime with [`Request`](crate::http::Request) and label parameters.
//! Additionally, the `handle_response_decision` function is called with the [`Response`](crate::http::Response) from
//! the interior service. The `handle_decision_feedback` function is called with both the
//! [`Response`](crate::http::Response) and the [`Verdict`] of the combined decision.
//!
//! If a handler function is not declared, the [`bulwark_plugin`] macro will automatically provide a no-op
//! implementation instead.
//!
//! The `handle_request_decision` and `handle_response_decision` functions return a [`HandlerOutput`] result.

#![allow(clippy::missing_safety_doc)]

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
pub mod http;
pub mod redis;

pub use bulwark_decision::*;
pub use errors::*;
pub use host_api::*;

/// Make wit_bindgen accessible to the macro crate, ensuring that the correct version is used.
#[doc(hidden)]
pub use wit_bindgen;
