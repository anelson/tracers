#![deny(warnings)]

pub use probers_core::{Tracer,Provider,ProviderProbe,ProviderBuilder};

pub type SystemTracer = probers_stap::StapTracer;
pub type SystemProvider = <SystemTracer as Tracer>::ProviderType;
pub type SystemProbe = <SystemTracer as Tracer>::ProbeType;
