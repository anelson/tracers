#![deny(warnings)]
#![recursion_limit = "256"]

use crate::spec::ProbeCallSpecification;
use crate::spec::ProviderInitSpecification;
use crate::spec::ProviderSpecification;
use failure::{format_err, Fallible};
use proc_macro2::Span;
use proc_macro2::TokenStream;
use std::env;
use std::io::Write;
use std::io::{stderr, stdout};
use std::path::{Path, PathBuf};

mod argtypes;
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
            span: span,
        }
    }
}

pub type ProberResult<T> = std::result::Result<T, ProberError>;

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

//On x86_04 linux, use the system tap tracer
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
pub type Generator = gen::dynamic::DynamicGenerator;

//On all other targets, use the no-op tracer
#[cfg(not(any(all(target_arch = "x86_64", target_os = "linux"))))]
pub type Generator = gen::noop::NoOpGenerator;

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
