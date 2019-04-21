#![deny(warnings)]

/// The code generated by `probers-macros` will at runtime require some functionality, both from
/// within this crate but also from third-party crates like `failure`.  It's important that the
/// generated code use _our_ version of these crates, and not be required to add some explicit
/// dependency itself.  So we'll re-export those dependencies here
/// Re-export our two dependencies that are actually used by code in user crates generated by
/// `probers!` macro.  By re-exporting the crate and not just the types, we ensure the correct
/// version will be used and spare the user having to add these dependencies themselves.  A deeper
/// discussion around this is ongoing right now at:
/// https://github.com/rust-lang-nursery/api-guidelines/issues/176
pub mod runtime {
    pub use probers_core::*;
    pub extern crate failure;
    pub extern crate once_cell;

    // Re-export some types from child crates which callers will need to be able to use.  Ergonomically
    // it makes more sense to a caller to deal with, for example, `probers::Provider`

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

}

#[cfg(test)]
mod test {
    use super::runtime::*;

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
