#![deny(warnings)]
#![recursion_limit = "256"]

use crate::build_rs::BuildInfo;
use crate::spec::ProbeCallSpecification;
use crate::spec::ProviderInitSpecification;
use crate::spec::ProviderSpecification;
use proc_macro2::TokenStream;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use strum_macros::AsRefStr;

mod argtypes;
pub mod build_rs;
mod cache;
mod cargo;
mod deps;
mod error;
pub mod gen;
mod hashing;
pub mod proc_macros;
pub mod spec;
mod syn_helpers;

pub use error::*;

pub use build_rs::build;

#[cfg(test)]
mod testdata;

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

/// Each probing implementation must implement this trait, which has components which are called at
/// build-time from `build.rs` and also components invoked by the macros at compile time.  Though
/// invoked in very different contexts, there is much overlap and thus it makes sense to provide
/// them all in one trait implementation.
pub trait CodeGenerator {
    /// Invoked by the `tracer` attribute macro to process a probing provider declaration and
    /// generate whatever code is required there.
    fn handle_provider_trait(&self, provider: ProviderSpecification) -> TracersResult<TokenStream>;

    /// Invoked by the `probe!` macro to (conditionally) fire a probe.
    fn handle_probe_call(&self, call: ProbeCallSpecification) -> TracersResult<TokenStream>;

    /// Invoked by the `init_provider!` macro to (optionally) initialize the provider, although one
    /// requirement of all implementations is that explicit initialization is not required and will
    /// be done lazily on first use.
    fn handle_provider_init(&self, init: ProviderInitSpecification) -> TracersResult<TokenStream>;

    /// This is invoked from within `build.rs` of the crate which is dependent upon `tracers`.  It
    /// doesn't take much arguments because it interacts directly with cargo via environment
    /// variables and stdout/stderr.
    ///
    /// It is designed not to panic; if there is a hard stop that should cause the dependent crate
    /// to fail, then it returns an error.  Most errors won't be hard stops, but merely warnings
    /// that cause the probing system to switch to a no-nop implementation
    fn generate_native_code(
        &self,
        stdout: &mut dyn Write,
        stderr: &mut dyn Write,
        manifest_dir: &Path,
        package_name: &str,
        targets: Vec<PathBuf>,
    ) -> TracersResult<()>;
}

/// Loads the `BuildInfo` and based on its contents creates and returns the applicable
/// `CodeGenerator` implementation
fn code_generator() -> TracersResult<Box<dyn CodeGenerator>> {
    let bi = BuildInfo::load()?;

    Ok(match bi.implementation {
        TracingImplementation::Disabled => Box::new(gen::disabled::DisabledGenerator::new(bi)),
        TracingImplementation::DynamicNoOp | TracingImplementation::DynamicStap => {
            Box::new(gen::dynamic::DynamicGenerator::new(bi))
        }
        TracingImplementation::NativeNoOp => Box::new(gen::native::noop::NoOpGenerator::new(bi)),
    })
}
