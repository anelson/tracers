#![deny(warnings)]

use proc_macro_hack::proc_macro_hack;

pub use probers_core::{Provider, ProviderBuilder, ProviderProbe, Tracer};

pub use probers_macros::prober;

//Alias `SystemTracer` to the appropriate implementation based on the target OS
//This is messy and I wish cargo offered us something more elegant.
//
//Note that the dependencies in `Cargo.toml` must perfectly align with these conditionals to
//ensure the correct implementation crate is in fact a dependency.
//
//On x86_04 linux, use the system tap tracer
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
pub type SystemTracer = probers_stap::StapTracer;
//On all other targets, use the no-op tracer
#[cfg(not(any(all(target_arch = "x86_64", target_os = "linux"))))]
pub type SystemTracer = probers_noop::NoOpTracer;

pub type SystemProvider = <SystemTracer as Tracer>::ProviderType;
pub type SystemProbe = <SystemTracer as Tracer>::ProbeType;

#[proc_macro_hack]
pub use probers_macros::probe;
