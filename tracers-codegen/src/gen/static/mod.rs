//! This module contains the code to support the static style of probing, which is more like the
//! static C support for user-mode tracing used by DTrace, SystemTap USDT, and DTrace USDT.  This
//! uses custom code running in the caller's `build.rs` to generate and compiler small C++ stubs
//! for each provider, which are implementing using the target tracing platform's own macros.  This
//! has a number of advantages over the implementation in `dynamic`:
//!
//! * The probes are embedded in the resulting binary, and can be discovered by system tools like
//! `tplist`, `bpftrace`, etc.
//! * This is the way the platform-specific tracing systems are intended to work.  Libraries that
//! implement dynamic tracing do so in a way that is, at best, a hack, and is poorly supported by
//! the static tools.
//! * This is much faster.  The Rust code will still incurr one method call when a probe is
//! enabled, which would not be incurred by equivalent static code.  But otherwise it's identical
//! to the static code implementation.  In limited cases when cross-language LTO is possible, the
//! optimizer can even remove this method call and produce the same performance as static C code.
//!
//! The only reason the `dynamic` implementation exists is that I wrote it first, before I figured
//! out how to make `static` work reliable.

use super::NativeLib;
use crate::build_rs::BuildInfo;
use crate::gen::common;
use crate::spec::ProbeCallSpecification;
use crate::spec::ProviderInitSpecification;
use crate::spec::ProviderSpecification;
use crate::{gen::CodeGenerator, TracersResult};
use proc_macro2::TokenStream;
use std::io::Write;
use std::path::{Path, PathBuf};

mod native_code;
mod probe_call;
mod provider_trait;

pub(crate) struct StaticGenerator {
    build_info: BuildInfo,
}

impl StaticGenerator {
    pub fn new(build_info: BuildInfo) -> StaticGenerator {
        StaticGenerator { build_info }
    }
}

impl CodeGenerator for StaticGenerator {
    fn handle_provider_trait(&self, provider: ProviderSpecification) -> TracersResult<TokenStream> {
        let generator = provider_trait::ProviderTraitGenerator::new(&self.build_info, provider);

        generator.generate()
    }

    fn handle_probe_call(&self, call: ProbeCallSpecification) -> TracersResult<TokenStream> {
        probe_call::generate_probe_call(&self.build_info, call)
    }

    fn handle_init_provider(&self, init: ProviderInitSpecification) -> TracersResult<TokenStream> {
        common::generate_init_provider(init)
    }

    fn generate_native_code(
        &self,
        stdout: &mut dyn Write,
        manifest_dir: &Path,
        out_dir: &Path,
        package_name: &str,
        targets: Vec<PathBuf>,
    ) -> Vec<NativeLib> {
        //Native code gen is only used for static, not for disabled
        if self.build_info.implementation.is_static() {
            native_code::generate_native_code(
                &self.build_info,
                stdout,
                manifest_dir,
                out_dir,
                package_name,
                targets,
            )
        } else {
            //When disabled, there's by definition no native code generated and thus no libs
            vec![]
        }
    }
}
