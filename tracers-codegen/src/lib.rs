#![deny(warnings)]
#![recursion_limit = "256"]

use serde::{Deserialize, Serialize};
use strum_macros::AsRefStr;

mod argtypes;
mod build_rs;
mod cache;
mod cargo;
mod deps;
mod error;
mod gen;
mod hashing;
pub mod proc_macros;
mod spec;
mod syn_helpers;
#[cfg(test)]
mod testdata;

//Export some of the internal types from their (private) modules
pub use build_rs::{build, tracers_build};
pub use error::*;

/// The available tracing implementations
#[derive(Debug, AsRefStr, Serialize, Deserialize, PartialEq)]
pub enum TracingImplementation {
    #[strum(serialize = "disabled")]
    Disabled,

    #[strum(serialize = "native_noop")]
    NativeNoOp,

    #[strum(serialize = "dyn_stap")]
    DynamicStap,

    #[strum(serialize = "dyn_noop")]
    DynamicNoOp,
}

impl TracingImplementation {
    pub fn is_enabled(&self) -> bool {
        *self != TracingImplementation::Disabled
    }

    pub fn is_dynamic(&self) -> bool {
        match self {
            TracingImplementation::DynamicNoOp | TracingImplementation::DynamicStap => true,
            TracingImplementation::Disabled | TracingImplementation::NativeNoOp => false,
        }
    }

    pub fn is_native(&self) -> bool {
        !self.is_dynamic()
    }
}
