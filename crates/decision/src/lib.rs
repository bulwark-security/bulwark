//! This crate provides the [`Decision`] implementation of Dempster-Shafer theory.

mod decision;
mod errors;

pub use decision::*;
pub use errors::*;

#[cfg(test)]
#[allow(unused_imports)]
#[macro_use]
extern crate approx;
