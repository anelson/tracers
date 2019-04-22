#![deny(warnings)]
#![recursion_limit = "256"]

use crate::build_rs::BuildInfo;
use crate::spec::ProbeCallSpecification;
use crate::spec::ProviderInitSpecification;
use crate::spec::ProviderSpecification;
use failure::{format_err, Fallible};
use proc_macro2::Span;
use proc_macro2::TokenStream;
use serde::{Deserialize, Serialize};
use std::env;
use std::io::Write;
use std::io::{stderr, stdout};
use std::path::{Path, PathBuf};
use strum_macros::AsRefStr;

mod argtypes;
pub mod build_rs;
mod cache;
mod cargo;
mod deps;
pub mod gen;
mod hashing;
pub mod proc_macros;
pub mod spec;
mod syn_helpers;

#[cfg(test)]
mod testdata;

#[derive(Debug)]
pub struct ProberError {
    pub message: String,
    pub span: Span,
}

impl PartialEq<ProberError> for ProberError {
    fn eq(&self, other: &ProberError) -> bool {
        self.message == other.message
    }
}

impl ProberError {
    fn new<M: ToString>(message: M, span: Span) -> ProberError {
        ProberError {
            message: message.to_string(),
            span,
        }
    }

    fn from_error(e: failure::Error) -> ProberError {
        ProberError {
            message: e.to_string(),
            span: Span::call_site(),
        }
    }
}

pub type ProberResult<T> = std::result::Result<T, ProberError>;

/// The available tracing implementations
#[derive(Debug, AsRefStr, Serialize, Deserialize)]
pub enum TracingImplementation {
    #[strum(serialize = "native_noop")]
    NativeNoOp,

    #[strum(serialize = "dyn_stap")]
    DynamicStap,

    #[strum(serialize = "dyn_noop")]
    DynamicNoOp,
}

impl TracingImplementation {
    pub fn is_dynamic(&self) -> bool {
        match self {
            TracingImplementation::DynamicNoOp | TracingImplementation::DynamicStap => true,
            TracingImplementation::NativeNoOp => false,
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
    /// Invoked by the `prober` attribute macro to process a probing provider declaration and
    /// generate whatever code is required there.
    fn handle_provider_trait(provider: ProviderSpecification) -> ProberResult<TokenStream>;

    /// Invoked by the `probe!` macro to (conditionally) fire a probe.
    fn handle_probe_call(call: ProbeCallSpecification) -> ProberResult<TokenStream>;

    /// Invoked by the `init_provider!` macro to (optionally) initialize the provider, although one
    /// requirement of all implementations is that explicit initialization is not required and will
    /// be done lazily on first use.
    fn handle_provider_init(init: ProviderInitSpecification) -> ProberResult<TokenStream>;

    /// This is invoked from within `build.rs` of the crate which is dependent upon `probers`.  It
    /// doesn't take much arguments because it interacts directly with cargo via environment
    /// variables and stdout/stderr.
    ///
    /// It is designed not to panic; if there is a hard stop that should cause the dependent crate
    /// to fail, then it returns an error.  Most errors won't be hard stops, but merely warnings
    /// that cause the probing system to switch to a no-nop implementation
    fn generate_native_code<WOut: Write, WErr: Write>(
        stdout: &mut WOut,
        stderr: &mut WErr,
        manifest_dir: &Path,
        package_name: &str,
        targets: Vec<PathBuf>,
    ) -> Fallible<()>;
}

/// Implementation of `CodeGenerator` which delegates to the actual generator which corresponds to
/// the implementation selected at build time and saved to disk somewhere in `$OUT_DIR` using the
/// `BuildInfo` struct
pub struct GeneratorSwitcher {}

/// A little macro to avoid excessive repetition.  Evaluates to an expression which calls `$method`
/// with `$args` on the `CodeGenerator` implementation which correponds to the implementation
/// returned by `choose_impl`
macro_rules! with_impl {
    ($method:ident ( $($args:expr),* ) ) => {
        with_impl!(choose_impl(), $method ( $($args),* ) )
    };
    ($choose_impl:expr, $method:ident ( $($args:expr),* ) ) => {
        $choose_impl.and_then(|imp| {
            match imp {
                TracingImplementation::NativeNoOp => gen::noop::NoOpGenerator::$method($($args),*),
                TracingImplementation::DynamicNoOp | TracingImplementation::DynamicStap => gen::dynamic::DynamicGenerator::$method($($args),*),
            }
        })
    };
}

fn choose_impl() -> ProberResult<TracingImplementation> {
    let bi = BuildInfo::load().map_err(|e| ProberError::from_error(e))?;

    Ok(bi.implementation)
}

impl CodeGenerator for GeneratorSwitcher {
    fn handle_provider_trait(provider: ProviderSpecification) -> ProberResult<TokenStream> {
        with_impl!(handle_provider_trait(provider))
    }

    fn handle_probe_call(call: ProbeCallSpecification) -> ProberResult<TokenStream> {
        with_impl!(handle_probe_call(call))
    }

    fn handle_provider_init(init: ProviderInitSpecification) -> ProberResult<TokenStream> {
        with_impl!(handle_provider_init(init))
    }

    fn generate_native_code<WOut: Write, WErr: Write>(
        stdout: &mut WOut,
        stderr: &mut WErr,
        manifest_dir: &Path,
        package_name: &str,
        targets: Vec<PathBuf>,
    ) -> Fallible<()> {
        // Until we refactor ProberError to be compatible with `failure`-based errors, we need this
        // hackery
        let chosen_impl = choose_impl().map_err(|e| format_err!("{}", e.message));
        with_impl!(
            chosen_impl,
            generate_native_code(stdout, stderr, manifest_dir, package_name, targets)
        )
    }
}

// Any other code that needs to refer to the current code generator impl does so through this type
// alias.
pub type Generator = GeneratorSwitcher;

pub fn build() {
    match build_internal() {
        Ok(_) => println!("probes build succeeded"),
        Err(e) => eprintln!("Error building probes: {}", e),
    }
}

pub fn build_internal() -> Fallible<()> {
    let manifest_path = env::var("CARGO_MANIFEST_DIR").map_err(|_| {
        format_err!(
            "CARGO_MANIFEST_DIR is not set; are you sure you're calling this from within build.rs?"
        )
    })?;
    let package_name = env::var("CARGO_PKG_NAME").unwrap();
    let targets = cargo::get_targets(&manifest_path, &package_name)?;

    let stdout = stdout();
    let stderr = stderr();

    let mut outhandle = stdout.lock();
    let mut errhandle = stderr.lock();

    Generator::generate_native_code(
        &mut outhandle,
        &mut errhandle,
        &Path::new(&manifest_path),
        &package_name,
        targets,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
