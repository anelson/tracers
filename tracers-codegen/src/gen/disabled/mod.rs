//! This module is very similar to the `native::noop` generator, except that when tracing is
//! disabled entirely there is no dependency on `tracers` at all, which means no runtime components
//! at all.  `noop` still uses the runtime code which implements wrapping of Rust types into C
//! types, although it uses them only at compile time it still requires that the user's crate have
//! a `probers` dependency.
use crate::build_rs::BuildInfo;
use crate::spec::ProbeCallSpecification;
use crate::spec::ProviderInitSpecification;
use crate::spec::ProviderSpecification;
use crate::{gen::CodeGenerator, TracersResult};
use proc_macro2::TokenStream;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::gen::native::noop::probe_call;
use crate::gen::native::noop::provider_trait;

#[allow(dead_code)]
pub(crate) struct DisabledGenerator {
    build_info: BuildInfo,
}

impl DisabledGenerator {
    pub fn new(build_info: BuildInfo) -> DisabledGenerator {
        DisabledGenerator { build_info }
    }
}

impl CodeGenerator for DisabledGenerator {
    fn handle_provider_trait(&self, provider: ProviderSpecification) -> TracersResult<TokenStream> {
        provider_trait::ProviderTraitGenerator::new(false, provider).generate()
    }

    fn handle_probe_call(&self, call: ProbeCallSpecification) -> TracersResult<TokenStream> {
        probe_call::generate_probe_call(call)
    }

    fn handle_provider_init(&self, _init: ProviderInitSpecification) -> TracersResult<TokenStream> {
        unimplemented!()
    }

    fn generate_native_code(
        &self,
        stdout: &mut dyn Write,
        _stderr: &mut dyn Write,
        _manifest_dir: &Path,
        _package_name: &str,
        _targets: Vec<PathBuf>,
    ) -> TracersResult<()> {
        // The nice thing about this implementation is that no build-time code generation is
        // required
        let _ = writeln!(
            stdout,
            "disabled generator doesn't require any build.rs code generation"
        );

        Ok(())
    }
}
