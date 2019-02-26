//! This module defines the API which is to be implemented by various tracing implementations to register and fire probes.
//! This API is not intended to be called directly but rather by the macros which wrap it in a type-safe facade.
use super::argtypes::{
    wrap, CType, ProbeArgNativeType, ProbeArgNativeTypeInfo, ProbeArgType, ProbeArgWrapper,
};
use failure::Fallible;
use std::marker::PhantomData;

/// Each implementation of the tracing API provides a `Tracer` implementation, which provides
/// tracing functionality for an entire process.
pub trait Tracer: Sized {
    type ProviderBuilderType: ProviderBuilder<Self>;
    type ProviderType: Provider<Self>;
    type ProbeType: UnsafeProviderProbeImpl;

    fn define_provider(
        name: &str,
        f: impl FnOnce(Self::ProviderBuilderType) -> Self::ProviderBuilderType,
    ) -> Fallible<Self::ProviderType>;
}

pub trait ProviderBuilder<TracerT: Tracer> {
    fn add_probe(&mut self, definition: &ProbeDefinition) -> Fallible<()>;
    fn build(self, name: &str) -> Fallible<<TracerT as Tracer>::ProviderType>;
}

pub trait Provider<TracerT: Tracer> : Sync + Drop {
    fn get_probe<ArgsT: ProbeArgs<ArgsT>>(
        &self,
        definition: &ProbeDefinition,
    ) -> Fallible<ProviderProbe<<TracerT as Tracer>::ProbeType, ArgsT>> {
        let unsafe_impl = self.get_probe_unsafe(definition)?;
        Ok(ProviderProbe::new(unsafe_impl))
    }

    fn get_probe_unsafe(
        &self,
        definition: &ProbeDefinition,
    ) -> Fallible<<TracerT as Tracer>::ProbeType>;
}

/// Holds a reference to the internal tracing implementation's probe structure,
/// and exposes a high-level type-safe API to fire the probe at will.
pub struct ProviderProbe<ImplT: UnsafeProviderProbeImpl, ArgsT: ProbeArgs<ArgsT>> {
    unsafe_probe_impl: ImplT,
    _args: PhantomData<ArgsT>,
}

impl<ImplT: UnsafeProviderProbeImpl, ArgsT: ProbeArgs<ArgsT>> ProviderProbe<ImplT, ArgsT> {
    fn new(probe: ImplT) -> Self {
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
    pub fn fire(&mut self, args: ArgsT) -> () {
        args.fire_probe(&mut self.unsafe_probe_impl)
    }
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
    fn fire_probe<ImplT: UnsafeProviderProbeImpl>(self, probe: &mut ImplT) -> ();
}

/// This structure is a runtime representation of a probe's definition.
/// A probe is defined by its name and the number and type of its arguments.  This is the
/// type used at runtime to identify probes and provide run-time type safety before allowing
/// probes to be co-erced into specific types.
#[derive(Clone, Debug, PartialEq, Hash, Eq)]
pub struct ProbeDefinition {
    pub name: String,
    pub arg_types: Vec<CType>,
}

impl ProbeDefinition {
    pub fn new<ArgsT: ProbeArgs<ArgsT>>(name: String) -> ProbeDefinition {
        ProbeDefinition {
            name: name,
            arg_types: <ArgsT as ProbeArgs<ArgsT>>::arg_types(),
        }
    }
}

/// Internal helper func used by the generated implementation to evaluate a probe arg type to its corresponding
/// CType enum value
fn get_ctype<T: ProbeArgType<T>>() -> CType {
    <<<T as ProbeArgType<T>>::WrapperType as ProbeArgWrapper>::CType as ProbeArgNativeTypeInfo>::get_c_type()
}

/// This macro helps implementors of this API implement the UnsafeProviderProbeImpl trait without having to
/// manually construct 13 different methods.  It should be used within an implementation of UnsafeProviderProbeImpl
/// which otherwise has no implementations for any of the `fireN` methods.
///
/// Example:
///
/// ```noexec
/// pub struct MyUnsafeProviderProbe{}
///
/// impl UnsafeProviderProbeImpl for MyUnsafeProviderProbe {
///     fn is_enabled(&self) -> bool { true }
///
///     impl_unsafe_provider_probe!(args, {
///         let parameters = vec![args];
///
///         let as_string = parameters.iter().map()
///
///         println!()
///     })
/// }
/// ```
//macro_rules! impl_unsafe_provider_probe {
//
//}

// The implementation of `ProbeArgs<T>` is provided for all tuples from
// 1to 12 elements.  That's highly repetitive, so there's code in `build.rs` that generates it.
// Here we need only include it:
include!(concat!(env!("OUT_DIR"), "/probe_args.rs"));

#[cfg(test)]
mod test {
    use super::*;
    use crate::argtypes::ProbeArgNativeType;
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;

    const BUFFER_SIZE: usize = 8192;

    /// A simple implementation of UnsafeProviderProbeImpl which uses `sprintf` to format the parameters
    /// and saves them as a Rust string for later verification.
    ///
    /// Note that you don't see the UnsafeProviderProbeImpl implementation here; it's in the `probe_args_tests.rs` file
    /// generated by the build and included below.
    struct TestingProviderProbeImpl {
        pub is_enabled: bool,
        pub format_string: CString,
        pub buffer: Vec<c_char>,
        pub calls: Vec<String>,
    }

    impl TestingProviderProbeImpl {
        pub fn new(format_string: String) -> TestingProviderProbeImpl {
            let c_format_string = CString::new(format_string).expect("invalid string");

            TestingProviderProbeImpl {
                is_enabled: false,
                format_string: c_format_string,
                buffer: Vec::with_capacity(BUFFER_SIZE),
                calls: Vec::new(),
            }
        }

        /// Called internally when the `buffer` member has been filled with a new string with `snprintf`.
        /// Convert that into a Rust string and add it to the `calls` vector.
        fn log_call(&mut self) {
            let cstring = unsafe { CStr::from_ptr(self.buffer.as_ptr()) };
            let as_str = cstring.to_str().expect("snprintf string isn't valid UTF-8");

            self.calls.push(as_str.to_string());
        }
    }

    fn make_test_probe<ArgsT: ProbeArgs<ArgsT>>(
        fmt_string: String,
    ) -> ProviderProbe<TestingProviderProbeImpl, ArgsT> {
        let unsafe_impl = TestingProviderProbeImpl::new(fmt_string);
        ProviderProbe::new(unsafe_impl)
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
        let mut probe_impl = make_test_probe("hey the probe fired".to_string());
        probe_impl.fire(());

        assert_eq!(
            probe_impl.unsafe_probe_impl.calls,
            vec!["hey the probe fired"]
        );
    }
}
