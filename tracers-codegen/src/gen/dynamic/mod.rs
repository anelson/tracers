//! The `dynamic` generator generates probing code which uses the runtime tracing API in
//! `tracers-core`.  Currently there is only one real implementation of that API, which uses
//! `libstapsdt` underneath to support creating SystemTap user-mode probes on 64-bit x86 Linux.
//! However other implementations using DTrace's equivalent library are also possible.
//!
//! This "dynamic" style was the first tracing mode supported in this library, but if I were to
//! write this crate over again I would never implement this mode.  The `static` style of probing
//! does more work at compile time and fits much better in the SystemTap/DTrace/ETW style of
//! tracing.  However, this remains in case a use for it emerges, perhaps on another platform with
//! more intrinsic support for dynamic style tracing.
use crate::build_rs::BuildInfo;
use crate::gen::common;
use crate::spec::ProbeCallSpecification;
use crate::spec::ProviderInitSpecification;
use crate::spec::ProviderSpecification;
use crate::{gen::CodeGenerator, TracersResult};
use proc_macro2::TokenStream;
use std::io::Write;
use std::path::{Path, PathBuf};

mod probe_call;
mod provider_trait;

pub(crate) struct DynamicGenerator {
    build_info: BuildInfo,
}

impl DynamicGenerator {
    pub fn new(build_info: BuildInfo) -> DynamicGenerator {
        DynamicGenerator { build_info }
    }
}

impl CodeGenerator for DynamicGenerator {
    fn handle_provider_trait(&self, provider: ProviderSpecification) -> TracersResult<TokenStream> {
        let generator = provider_trait::ProviderTraitGenerator::new(&self.build_info, provider);

        generator.generate()
    }

    fn handle_probe_call(&self, call: ProbeCallSpecification) -> TracersResult<TokenStream> {
        probe_call::generate_probe_call(call)
    }

    fn handle_init_provider(&self, init: ProviderInitSpecification) -> TracersResult<TokenStream> {
        common::generate_init_provider(init)
    }

    fn generate_native_code(
        &self,
        stdout: &mut dyn Write,
        _manifest_dir: &Path,
        _out_dir: &Path,
        _package_name: &str,
        _targets: Vec<PathBuf>,
    ) {
        // The nice thing about this implementation is that no build-time code generation is
        // required
        let _ = writeln!(
            stdout,
            "dynamic generator doesn't require any build.rs code generation"
        );
    }
}
