//! This module contains the dynamic tracing API which is implemented by various platform-specific
//! providers.
//!
//! The dynamic tracing API is designed for platforms that do not have or projects that do not want
//! to use static tracing, which is usually more performant.  Dynamic tracing is implemented in
//! libraries like `libstapsdt`.

use crate::argtypes::{
    wrap, CType, ProbeArgNativeType, ProbeArgNativeTypeInfo, ProbeArgType, ProbeArgWrapper,
};
use failure::Fallible;
use std::marker::PhantomData;

/// Each implementation of the tracing API provides a `Tracer` implementation, which provides
/// tracing functionality for an entire process.
pub trait Tracer: Sized {
    const TRACING_IMPLEMENTATION: &'static str;

    /// The type used to construct tracing providers
    type ProviderBuilderType: ProviderBuilder<Self>;

    /// The type which represents a tracing provider.  This type must always be `Send` and `Sync`
    /// because there is no restriction placed on use of and sharing between multiple threads.
    type ProviderType: Provider<Self> + Sync + Send;

    /// The type which represents a tracing probe.  This type must always be `Send` and `Sync`
    /// because there is no restriction placed on use of and sharing between multiple threads.
    type ProbeType: UnsafeProviderProbeImpl + Sync + Send;

    fn define_provider(
        name: &str,
        f: impl FnOnce(Self::ProviderBuilderType) -> Fallible<Self::ProviderBuilderType>,
    ) -> Fallible<Self::ProviderType>;
}

pub trait ProviderBuilder<TracerT: Tracer> {
    fn add_probe<ArgsT: ProbeArgs<ArgsT>>(&mut self, name: &'static str) -> Fallible<()>;
    fn build(self, name: &str) -> Fallible<<TracerT as Tracer>::ProviderType>;
}

pub trait Provider<TracerT: Tracer> {
    fn get_probe<ArgsT: ProbeArgs<ArgsT>>(
        &self,
        name: &'static str,
    ) -> Fallible<ProviderProbe<<TracerT as Tracer>::ProbeType, ArgsT>> {
        let definition = ProbeDefinition::new::<ArgsT>(name);
        let unsafe_impl = self.get_probe_unsafe(&definition)?;
        Ok(ProviderProbe::new(unsafe_impl))
    }

    fn get_probe_unsafe(
        &self,
        definition: &ProbeDefinition,
    ) -> Fallible<&<TracerT as Tracer>::ProbeType>;
}

/// Holds a reference to the internal tracing implementation's probe structure,
/// and exposes a high-level type-safe API to fire the probe at will.
#[derive(Copy, Clone)]
pub struct ProviderProbe<'probe, ImplT: UnsafeProviderProbeImpl, ArgsT: ProbeArgs<ArgsT>> {
    unsafe_probe_impl: &'probe ImplT,
    _args: PhantomData<ArgsT>,
}

impl<'probe, ImplT: UnsafeProviderProbeImpl, ArgsT: ProbeArgs<ArgsT>>
    ProviderProbe<'probe, ImplT, ArgsT>
{
    fn new(probe: &'probe ImplT) -> Self {
        ProviderProbe {
            unsafe_probe_impl: probe,
            _args: PhantomData,
        }
    }

    /// Indicates if this probe is currently enabled; that is, if there are one or more processes
    /// monitoring this probe.  This call should be very fast, in many cases only a memory access,
    /// and thus can be used even in production and performance-sensitive code.
    pub fn is_enabled(&self) -> bool {
        self.unsafe_probe_impl.is_enabled()
    }

    /// Fires the probe.  Note that it's assumed higher level code has tested `is_enabled()` already.
    /// If the probe isnt' enabled, it's not an error to attempt to fire it, just a waste of cycles.
    pub fn fire(&self, args: ArgsT) {
        args.fire_probe(self.unsafe_probe_impl)
    }
}

// If the underlying impl is Sync/Send, then so is this wrapper around it

unsafe impl<'probe, ImplT: UnsafeProviderProbeImpl + Sync, ArgsT: ProbeArgs<ArgsT>> Sync
    for ProviderProbe<'probe, ImplT, ArgsT>
{
}
unsafe impl<'probe, ImplT: UnsafeProviderProbeImpl + Send, ArgsT: ProbeArgs<ArgsT>> Send
    for ProviderProbe<'probe, ImplT, ArgsT>
{
}

/// All tuple types which consist entirely of elements which have a `ProbeArgType<T>` implementation
/// also implement `ProbeArgs<T>` where `T` is the tuple type.  This provides auto-generated methods
/// to obtain runtime information about the argument types, and also to wrap the elements and pass them
/// to the underlying C probing implementation in a typesafe manner.
pub trait ProbeArgs<T> {
    /// The arity of this tuple
    const ARG_COUNT: usize;

    /// A vector consisting of the CType enum corresponding to the C type which represents each element
    /// in the tuple.
    fn arg_types() -> Vec<CType>;

    /// Converts all of the probe args in this tuple to their C representations and passes them to the
    /// underlying UnsafeProviderProbeImpl implementation.
    fn fire_probe<ImplT: UnsafeProviderProbeImpl>(self, probe: &ImplT);
}

/// This structure is a runtime representation of a probe's definition.
/// A probe is defined by its name and the number and type of its arguments.  This is the
/// type used at runtime to identify probes and provide run-time type safety before allowing
/// probes to be co-erced into specific types.
#[derive(Clone, Debug, PartialEq, Hash, Eq)]
pub struct ProbeDefinition {
    pub name: &'static str,
    pub arg_types: Vec<CType>,
}

impl ProbeDefinition {
    pub fn new<ArgsT: ProbeArgs<ArgsT>>(name: &'static str) -> ProbeDefinition {
        ProbeDefinition {
            name,
            arg_types: <ArgsT as ProbeArgs<ArgsT>>::arg_types(),
        }
    }
}

/// Internal helper func used by the generated implementation to evaluate a probe arg type to its corresponding
/// CType enum value
fn get_ctype<T: ProbeArgType<T>>() -> CType {
    <<<T as ProbeArgType<T>>::WrapperType as ProbeArgWrapper>::CType as ProbeArgNativeTypeInfo>::get_c_type()
}

// The implementation of `ProbeArgs<T>` is provided for all tuples from
// 1to 12 elements.  That's highly repetitive, so there's code in `build.rs` that generates it.
// Here we need only include it:
include!(concat!(env!("OUT_DIR"), "/probe_args.rs"));

#[cfg(test)]
#[cfg(unix)] //Windows doesn't have the same libc functions like snprintf...
mod test {
    use super::*;
    use crate::argtypes::ProbeArgNativeType;
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;
    use std::sync::Mutex;

    const BUFFER_SIZE: usize = 8192;

    /// A simple implementation of UnsafeProviderProbeImpl which uses `sprintf` to format the parameters
    /// and saves them as a Rust string for later verification.
    ///
    /// Note that you don't see the UnsafeProviderProbeImpl implementation here; it's in the `probe_args_tests.rs` file
    /// generated by the build and included below.
    struct TestingProviderProbeImpl {
        pub is_enabled: bool,
        pub format_string: CString,
        pub buffer: Mutex<Vec<c_char>>,
        pub calls: Mutex<Vec<String>>,
    }

    impl TestingProviderProbeImpl {
        pub fn new(format_string: String) -> TestingProviderProbeImpl {
            let c_format_string = CString::new(format_string).expect("invalid string");

            TestingProviderProbeImpl {
                is_enabled: false,
                format_string: c_format_string,
                buffer: Mutex::new(Vec::with_capacity(BUFFER_SIZE)),
                calls: Mutex::new(Vec::new()),
            }
        }

        /// Called internally when the `buffer` member has been filled with a new string with `snprintf`.
        /// Convert that into a Rust string and add it to the `calls` vector.
        fn log_call(&self) {
            let buffer = self.buffer.lock().unwrap();
            let mut calls = self.calls.lock().unwrap();
            let cstring = unsafe { CStr::from_ptr(buffer.as_ptr()) };
            let as_str = cstring.to_str().expect("snprintf string isn't valid UTF-8");

            calls.push(as_str.to_string());
        }

        fn get_calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    /// Convert a Rust string to a CString and then back again.  Some of the Quickcheck strings
    /// contains NULLs and thus fail this test and convert back as empty strings.  In that case
    /// we should expect an "(null)" string result not the original string, which is what this method
    /// provides.
    fn c_and_back_again(arg: &str) -> String {
        CString::new(arg)
            .map(|s| s.to_str().expect("Invalid UTF-8").to_string())
            .unwrap_or("(null)".to_string()) //when we pass a NULL to sprintf with %s fmt it outputs"(null)"
    }

    include!(concat!(env!("OUT_DIR"), "/probe_args_tests.rs"));

    #[test]
    fn test_fire0() {
        let unsafe_impl = TestingProviderProbeImpl::new("hey the probe fired".to_string());
        let probe_impl = ProviderProbe::new(&unsafe_impl);
        probe_impl.fire(());

        assert_eq!(
            probe_impl.unsafe_probe_impl.get_calls(),
            vec!["hey the probe fired"]
        );
    }
}
