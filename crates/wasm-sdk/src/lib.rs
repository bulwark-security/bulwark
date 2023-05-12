//! This crate provides the logic for Bulwark's guest environment.

// TODO: the host/guest wit files seem to be why the latest version switched to one generate macro?
// TODO: switch to wasmtime::component::bindgen!
wit_bindgen_rust::import!("../../bulwark-host.wit");

mod from;
mod host_calls;

pub use bulwark_decision::*;
pub use from::*;
pub use host_calls::*;

#[allow(unused_imports)]
#[macro_use]
extern crate approx;
