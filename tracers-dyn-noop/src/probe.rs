//! Implements the `UnsafeProviderProbeImpl` trait that does nothing ('no-op')

use tracers_core::{ProbeArgNativeType, UnsafeProviderProbeNativeImpl};

#[derive(Clone)]
pub struct NoOpProbe {}

// See the `Send` and `Sync` implementations for `NoOpProvider` for more explication

unsafe impl Sync for NoOpProbe {}
unsafe impl Send for NoOpProbe {}

// The implementation of UnsafeProviderProbeNativeImpl involves repetitive code for 13 different
// arities.  Thus, it's generated in `build.rs` not written manually
include!(concat!(env!("OUT_DIR"), "/probe_unsafe_impl.rs"));
