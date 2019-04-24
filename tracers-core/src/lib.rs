#![deny(warnings)]

#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;

//Re-export some third-party dependencies so the caller can be sure to use the exact version we use
//and doesn't have to add their own explicit dep
pub extern crate failure;

pub mod argtypes;
pub use argtypes::{wrap, ProbeArgNativeType, ProbeArgType, ProbeArgWrapper};

#[cfg(feature = "dynamic")]
pub mod dynamic;
