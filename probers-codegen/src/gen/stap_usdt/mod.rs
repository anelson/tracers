//! This is the code generation for the SystemTap USDT probing implementation, which internally
//! uses libstapstd under the covers to generate SystemTap user space probes at runtime.  It's not
//! a great solution because tools like `bpftrace` and the `bcc` family don't work very well with
//! these kinds of probes, but it's a beginning
use crate::provider::ProviderSpecification;
use crate::{CodeGenerator, ProberResult};
use proc_macro2::TokenStream;
use provider::ProviderGenerator;
use std::io::Write;

mod probe;
mod provider;

pub struct StapUsdtGenerator {}

impl CodeGenerator for StapUsdtGenerator {
    fn handle_provider_trait(trait_item: &syn::ItemTrait) -> ProberResult<TokenStream> {
        let spec = ProviderSpecification::from_trait(trait_item)?;
        let generator = ProviderGenerator::new(&spec);

        generator.generate()
    }

    fn handle_probe_call(_call: &syn::Expr) -> ProberResult<TokenStream> {
        unimplemented!()
    }

    fn handle_provider_init(_typ: &syn::TypePath) -> ProberResult<TokenStream> {
        unimplemented!()
    }

    fn generate_native_code<WOut: Write, WErr: Write>(
        _stdout: &mut WOut,
        _stderr: &mut WErr,
    ) -> ProberResult<TokenStream> {
        unimplemented!()
    }
}
