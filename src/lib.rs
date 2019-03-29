#![deny(warnings)]

use proc_macro_hack::proc_macro_hack;

pub use probers_core::{Provider, ProviderBuilder, ProviderProbe, Tracer};

pub use probers_macros::prober;

pub type SystemTracer = probers_stap::StapTracer;
pub type SystemProvider = <SystemTracer as Tracer>::ProviderType;
pub type SystemProbe = <SystemTracer as Tracer>::ProbeType;

#[proc_macro_hack]
pub use probers_macros::probe;
