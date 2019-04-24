//! This module implements the probing macros in such a way that at runtime they do nothing (hence
//! `noop` -- No Operation).  But they don't just compile down to nothing.
//!
//! It's important that even when the noop implementation is used, the same compile-time
//! verification applies as to the real implementatiosn.  Otherwise developers would do most of
//! their work with tracing disabled (meaning `noop`), then when they run into a problem that calls
//! for tracing, or do a release build with tracing enabled, they'd find their code is suddenly
//! broken.
//!
//! Thus this mode uses somme creative Rust trickery to generate code that ensures the compiler
//! does its usual type checks, but at runtime nothing actually happens.
use crate::spec::ProbeCallSpecification;
use crate::spec::ProviderInitSpecification;
use crate::spec::ProviderSpecification;
use crate::{CodeGenerator, TracersResult};
use proc_macro2::TokenStream;
use std::io::Write;
use std::path::{Path, PathBuf};

mod probe_call;
mod provider_trait;

#[allow(dead_code)]
pub struct NoOpGenerator {}

impl CodeGenerator for NoOpGenerator {
    fn handle_provider_trait(provider: ProviderSpecification) -> TracersResult<TokenStream> {
        provider_trait::ProviderTraitGenerator::new(provider).generate()
    }

    fn handle_probe_call(call: ProbeCallSpecification) -> TracersResult<TokenStream> {
        probe_call::generate_probe_call(call)
    }

    fn handle_provider_init(_init: ProviderInitSpecification) -> TracersResult<TokenStream> {
        unimplemented!()
    }

    fn generate_native_code<WOut: Write, WErr: Write>(
        stdout: &mut WOut,
        _stderr: &mut WErr,
        _manifest_dir: &Path,
        _package_name: &str,
        _targets: Vec<PathBuf>,
    ) -> TracersResult<()> {
        // The nice thing about this implementation is that no build-time code generation is
        // required
        let _ = writeln!(
            stdout,
            "no-op generator doesn't require any build.rs code generation"
        );

        Ok(())
    }
}
