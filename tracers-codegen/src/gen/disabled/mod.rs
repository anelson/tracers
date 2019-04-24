//! This module is very similar to the `native::noop` generator, except that when tracing is
//! disabled entirely there is no dependency on `tracers` at all, which means no runtime components
//! at all.  `noop` still uses the runtime code which implements wrapping of Rust types into C
//! types, although it uses them only at compile time it still requires that the user's crate have
//! a `probers` dependency.
use crate::build_rs::BuildInfo;
use crate::gen::native::noop::probe_call;
use crate::gen::native::noop::provider_trait;
use crate::spec::ProbeCallSpecification;
use crate::spec::ProviderInitSpecification;
use crate::spec::ProviderSpecification;
use crate::TracersError;
use crate::{gen::CodeGenerator, TracersResult};
use failure::format_err;
use proc_macro2::TokenStream;
use quote::quote_spanned;
use std::io::Write;
use std::path::{Path, PathBuf};
use syn::spanned::Spanned;

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
        provider_trait::ProviderTraitGenerator::new(&self.build_info, provider).generate()
    }

    fn handle_probe_call(&self, call: ProbeCallSpecification) -> TracersResult<TokenStream> {
        probe_call::generate_probe_call(call)
    }

    fn handle_init_provider(&self, init: ProviderInitSpecification) -> TracersResult<TokenStream> {
        generate_init_provider(init)
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

/// Generates the stripped-down version of the `init_provider` macro that doesn't assume any trait
/// code was generated at all
fn generate_init_provider(init: ProviderInitSpecification) -> TracersResult<TokenStream> {
    //Probing is entirely disabled so there is not provider `__try_init_provider`, evaluate
    //the macro to a literal
    let provider = init.provider;
    let span = provider.span();
    let provider_ident = provider
        .segments
        .last()
        .map(|pair| pair.value().ident.clone())
        .ok_or_else(|| {
            TracersError::other_error(format_err!("Error getting trait name").context(""))
        })?;

    let provider_name = ProviderSpecification::provider_name_from_trait(&provider_ident);
    let version = env!("CARGO_PKG_VERSION");
    Ok(quote_spanned! {span=>
        {
            ::core::result::Result::<&'static str, &'static str>::Ok(concat!(#provider_name, "::", "disabled", "::", #version))
        }
    })
}
