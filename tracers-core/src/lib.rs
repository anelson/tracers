#![deny(warnings)]

#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;

pub mod argtypes;
pub use argtypes::{wrap, ProbeArgNativeType, ProbeArgType, ProbeArgWrapper};

pub mod probes;
pub use probes::*;
