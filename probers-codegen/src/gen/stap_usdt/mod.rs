//! This is the code generation for the SystemTap USDT probing implementation, which internally
//! uses libstapstd under the covers to generate SystemTap user space probes at runtime.  It's not
//! a great solution because tools like `bpftrace` and the `bcc` family don't work very well with
//! these kinds of probes, but it's a beginning
use crate::probe_call::ProbeCall;
use crate::provider::ProviderSpecification;
use crate::provider_init::ProviderInitSpecification;
use crate::{CodeGenerator, ProberResult};
use proc_macro2::TokenStream;
use std::io::Write;

mod probe_call;
mod provider_init;
mod provider_trait;

pub struct StapUsdtGenerator {}

impl CodeGenerator for StapUsdtGenerator {
    fn handle_provider_trait(provider: ProviderSpecification) -> ProberResult<TokenStream> {
        let generator = provider_trait::ProviderTraitGenerator::new(provider);

        generator.generate()
    }

    fn handle_probe_call(call: ProbeCall) -> ProberResult<TokenStream> {
        probe_call::generate_probe_call(call)
    }

    fn handle_provider_init(init: ProviderInitSpecification) -> ProberResult<TokenStream> {
        provider_init::generate_provider_init(init)
    }

    fn generate_native_code<WOut: Write, WErr: Write>(
        _stdout: &mut WOut,
        _stderr: &mut WErr,
    ) -> ProberResult<TokenStream> {
        unimplemented!()
    }
}
