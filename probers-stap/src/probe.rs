//! Implements the `UnsafeProviderProbeImpl` trait for SystemTap
use libstapsdt_sys::{probeFire, SDTProbe_t};
use probers_core::{ProbeArgNativeType, UnsafeProviderProbeNativeImpl};

/// Represents a SystemTap probe, which is simply an `SDTProbe_t*`.  This can be copied very
/// cheaply which is why Clone is derived.
#[derive(Clone)]
pub struct StapProbe {
    pub(crate) probe: *mut SDTProbe_t,
}

impl StapProbe {
    #[inline(always)]
    fn is_enabled(&self) -> bool {
        unsafe {
            //This logic copied from the C code for `probeIsEnabled`
            let fire = (*self.probe)._fire as *const u8;

            if fire.is_null() {
                return false;
            };

            if ((*fire) & 0x90) == 0x90 {
                return false;
            }

            return true;
        }
    }
}

// See the `Send` and `Sync` implementations for `StapProvider` for more explication

unsafe impl Sync for StapProbe {}
unsafe impl Send for StapProbe {}

// The implementation of UnsafeProviderProbeNativeImpl involves repetitive code for 13 different
// arities.  Thus, it's generated in `build.rs` not written manually
include!(concat!(env!("OUT_DIR"), "/probe_unsafe_impl.rs"));
