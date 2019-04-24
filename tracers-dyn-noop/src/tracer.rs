//! This module implements the `Tracer` trait in such a way that the API calls all work but don't
//! do anything.  In release builds most probably the entire probing infrastructure will optimize
//! away, and even in debug builds the overhead from probing in this implementation will be just a
//! few instructions
use failure::Fallible;

use tracers_core::Tracer;

use super::{NoOpProbe, NoOpProvider, NoOpProviderBuilder};

pub struct NoOpTracer {}

impl Tracer for NoOpTracer {
    const TRACING_IMPLEMENTATION: &'static str = "no-op";

    type ProviderBuilderType = NoOpProviderBuilder;
    type ProviderType = NoOpProvider;
    type ProbeType = NoOpProbe;

    fn define_provider(
        _name: &str,
        f: impl FnOnce(Self::ProviderBuilderType) -> Fallible<Self::ProviderBuilderType>,
    ) -> Fallible<Self::ProviderType> {
        f(NoOpProviderBuilder::new())?;

        Ok(NoOpProvider {})
    }
}
