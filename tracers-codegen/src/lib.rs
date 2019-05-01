#![deny(warnings)]
#![recursion_limit = "512"]

use serde::{Deserialize, Serialize};
use std::str::FromStr;
use strum::EnumProperty;
use strum_macros::{AsRefStr, EnumProperty, EnumString};

mod argtypes;
mod build_rs;
mod cache;
mod cargo;
mod deps;
mod error;
mod gen;
mod hashing;
pub mod proc_macros;
mod serde_helpers;
mod spec;
mod syn_helpers;
#[cfg(test)]
mod testdata;

//Export some of the internal types from their (private) modules
pub use build_rs::{build, tracers_build};
pub use error::*;

/// The categories of tracing implementations.  Within `Static` and `Dynamic` there are various
/// platform-specific implementations, however the behavior of all implementations within a
/// category is broadly identical
#[derive(Debug, AsRefStr, Serialize, Deserialize, PartialEq, EnumString)]
pub(crate) enum TracingType {
    #[strum(serialize = "disabled")]
    Disabled,
    #[strum(serialize = "static")]
    Static,
    #[strum(serialize = "dynamic")]
    Dynamic,
}

impl TracingType {
    pub fn is_enabled(&self) -> bool {
        *self != TracingType::Disabled
    }
}

/// The possible platform-specific implementations of tracing, regardless of which type they are
#[derive(Debug, AsRefStr, Serialize, Deserialize, PartialEq, EnumString)]
pub(crate) enum TracingTarget {
    #[strum(serialize = "disabled")]
    Disabled,
    #[strum(serialize = "stap")]
    Stap,
    #[strum(serialize = "noop")]
    NoOp,
}

impl TracingTarget {
    pub fn is_enabled(&self) -> bool {
        *self != TracingTarget::Disabled && *self != TracingTarget::NoOp
    }
}

/// All possible tracing implementations.  Every supported linear combination of `TracingType` and
/// `TracingTarget`
#[derive(Clone, Debug, AsRefStr, Serialize, Deserialize, EnumProperty, PartialEq)]
pub(crate) enum TracingImplementation {
    #[strum(serialize = "disabled", props(type = "disabled", target = "disabled"))]
    Disabled,

    #[strum(serialize = "static_stap", props(type = "static", target = "stap"))]
    StaticStap,

    #[strum(serialize = "static_noop", props(type = "static", target = "noop"))]
    StaticNoOp,

    #[strum(serialize = "dyn_stap", props(type = "dynamic", target = "stap"))]
    DynamicStap,

    #[strum(serialize = "dyn_noop", props(type = "dynamic", target = "noop"))]
    DynamicNoOp,
}

impl TracingImplementation {
    pub fn tracing_type(&self) -> TracingType {
        TracingType::from_str(&*self.get_str("type").unwrap()).unwrap()
    }

    pub fn tracing_target(&self) -> TracingTarget {
        TracingTarget::from_str(&*self.get_str("target").unwrap()).unwrap()
    }

    pub fn is_enabled(&self) -> bool {
        self.tracing_type().is_enabled()
    }

    pub fn is_dynamic(&self) -> bool {
        self.tracing_type() == TracingType::Dynamic
    }

    pub fn is_static(&self) -> bool {
        self.tracing_type() == TracingType::Static
    }
}
