//! The `dynamic` generator generates probing code which uses the runtime tracing API in
//! `tracers-core`.  Currently there is only one real implementation of that API, which uses
//! `libstapsdt` underneath to support creating SystemTap user-mode probes on 64-bit x86 Linux.
//! However other implementations using DTrace's equivalent library are also possible.
//!
//! This "dynamic" style was the first tracing mode supported in this library, but if I were to
//! write this crate over again I would never implement this mode.  The `native` style of probing
//! does more work at compile time and fits much better in the SystemTap/DTrace/ETW style of
//! tracing.  However, this remains in case a use for it emerges, perhaps on another platform with
//! more intrinsic support for dynamic style tracing.
use crate::spec::ProbeCallSpecification;
use crate::spec::ProviderInitSpecification;
use crate::spec::ProviderSpecification;
use crate::{CodeGenerator, TracersResult};
use proc_macro2::TokenStream;
use std::io::Write;
use std::path::{Path, PathBuf};

mod probe_call;
mod provider_init;
mod provider_trait;

pub struct DynamicGenerator {}

impl CodeGenerator for DynamicGenerator {
    fn handle_provider_trait(provider: ProviderSpecification) -> TracersResult<TokenStream> {
        let generator = provider_trait::ProviderTraitGenerator::new(provider);

        generator.generate()
    }

    fn handle_probe_call(call: ProbeCallSpecification) -> TracersResult<TokenStream> {
        probe_call::generate_probe_call(call)
    }

    fn handle_provider_init(init: ProviderInitSpecification) -> TracersResult<TokenStream> {
        provider_init::generate_provider_init(init)
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
            "dynamic generator doesn't require any build.rs code generation"
        );
        Ok(())
    }
}
