//! This is the code generation for the SystemTap USDT probing implementation, which internally
//! uses libstapstd under the covers to generate SystemTap user space probes at runtime.  It's not
//! a great solution because tools like `bpftrace` and the `bcc` family don't work very well with
//! these kinds of probes, but it's a beginning
use crate::provider::ProviderSpecification;
use crate::{CodeGenerator, ProberResult};
use proc_macro2::TokenStream;
use std::io::Write;

mod probe_call;
mod provider_trait;

pub struct StapUsdtGenerator {}

impl CodeGenerator for StapUsdtGenerator {
    fn handle_provider_trait(trait_item: &syn::ItemTrait) -> ProberResult<TokenStream> {
        let spec = ProviderSpecification::from_trait(trait_item)?;
        let generator = provider_trait::ProviderTraitGenerator::new(&spec);

        generator.generate()
    }

    fn handle_probe_call(call: &syn::Expr) -> ProberResult<TokenStream> {
        probe_call::generate_probe_call(call)
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
