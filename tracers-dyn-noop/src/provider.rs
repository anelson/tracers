//! Implements the `ProviderBuilder` and `Provider` traits for SystemTap
use failure::Fallible;
use tracers_core::dynamic::{ProbeArgs, ProbeDefinition, Provider, ProviderBuilder};

use super::{NoOpProbe, NoOpTracer};

pub struct NoOpProviderBuilder {}

impl NoOpProviderBuilder {
    pub(crate) fn new() -> NoOpProviderBuilder {
        NoOpProviderBuilder {}
    }
}

impl ProviderBuilder<NoOpTracer> for NoOpProviderBuilder {
    fn add_probe<ArgsT: ProbeArgs<ArgsT>>(&mut self, _name: &'static str) -> Fallible<()> {
        Ok(())
    }

    fn build(self, _name: &str) -> Fallible<NoOpProvider> {
        Ok(NoOpProvider {})
    }
}

pub struct NoOpProvider {}

unsafe impl Sync for NoOpProvider {}

unsafe impl Send for NoOpProvider {}

static NO_OP_PROBE: NoOpProbe = NoOpProbe {};

impl Provider<NoOpTracer> for NoOpProvider {
    /// Look up the probe by its definition (that is, name and arg types)
    fn get_probe_unsafe(&self, _definition: &ProbeDefinition) -> Fallible<&NoOpProbe> {
        Ok(&NO_OP_PROBE)
    }
}
