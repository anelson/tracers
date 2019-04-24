//! This module is very similar to the `native::noop` generator, except that when tracing is
//! disabled entirely there is no dependency on `tracers` at all, which means no runtime components
//! at all.  `noop` still uses the runtime code which implements wrapping of Rust types into C
//! types, although it uses them only at compile time it still requires that the user's crate have
//! a `probers` dependency.
//!
//! This module implements the proc macro code generation in such a way that most of the type
//! checking and validation still happens, just without the additional assurance that there exists
//! a valid `ProbeArgType` wrapper for each probe argument type.
use crate::build_rs::BuildInfo;
use crate::spec::ProbeCallSpecification;
use crate::spec::ProviderInitSpecification;
use crate::spec::ProviderSpecification;
use crate::{CodeGenerator, TracersResult};
use proc_macro2::TokenStream;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct DisabledGenerator {
    _build_info: BuildInfo,
}

impl DisabledGenerator {
    pub fn new(build_info: BuildInfo) -> DisabledGenerator {
        DisabledGenerator {
            _build_info: build_info,
        }
    }
}

impl CodeGenerator for DisabledGenerator {
    fn handle_provider_trait(
        &self,
        _provider: ProviderSpecification,
    ) -> TracersResult<TokenStream> {
        unimplemented!()
    }

    fn handle_probe_call(&self, _call: ProbeCallSpecification) -> TracersResult<TokenStream> {
        unimplemented!()
    }

    fn handle_provider_init(&self, _init: ProviderInitSpecification) -> TracersResult<TokenStream> {
        unimplemented!()
    }

    fn generate_native_code(
        &self,
        _stdout: &mut dyn Write,
        _stderr: &mut dyn Write,
        _manifest_dir: &Path,
        _package_name: &str,
        _targets: Vec<PathBuf>,
    ) -> TracersResult<()> {
        unimplemented!()
    }
}
