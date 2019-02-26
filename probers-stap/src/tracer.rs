//! This module implements the `Tracer` trait in terms of the `libstapsdt-sys` crate; that is,
//! it implements `Tracer` for the Linux SystemTap tracing API
use failure::Fallible;

use probers_core::{ProviderBuilder, Tracer};

use super::{StapProbe, StapProvider, StapProviderBuilder};

pub struct StapTracer {}

impl Tracer for StapTracer {
    type ProviderBuilderType = StapProviderBuilder;
    type ProviderType = StapProvider;
    type ProbeType = StapProbe;

    fn define_provider(
        name: &str,
        f: impl FnOnce(Self::ProviderBuilderType) -> Fallible<Self::ProviderBuilderType>,
    ) -> Fallible<Self::ProviderType> {
        let builder = StapProviderBuilder::new();
        let builder = f(builder)?;
        let provider = builder.build(name)?;

        Ok(provider)
    }
}

impl Drop for StapTracer {
    fn drop(&mut self) {}
}
