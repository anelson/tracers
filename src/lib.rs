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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn verify_expected_tracing_impl() {
        //This very simple test checks the PROBERS_EXPECTED_IMPL env var, and if set, asserts that
        //the tracing implementation compiled into this library matches the expected one.  In
        //practice this is only used by the CI builds to verify that the compile-time magic always
        //ends up with the expeced implementation on a variety of environments
        if let Ok(expected_impl) = std::env::var("PROBERS_EXPECTED_IMPL") {
            assert_eq!(expected_impl, SystemTracer::TRACING_IMPLEMENTATION);
        }
    }
}
