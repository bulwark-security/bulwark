//! This crate provides the logic for Bulwark's guest environment.

mod host_calls;

pub use bulwark_decision::*;
pub use host_calls::*;

#[allow(unused_imports)]
#[macro_use]
extern crate approx;
