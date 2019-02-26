#![deny(warnings)]

#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;

pub mod argtypes;
pub use crate::argtypes::{wrap, ProbeArgType, ProbeArgWrapper, ProbeArgNativeType};

pub mod probes;
pub use probes::*;
