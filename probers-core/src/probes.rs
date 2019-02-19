//! This module defines the API which is to be implemented by various tracing implementations to register and fire probes.
//! This API is not intended to be called directly but rather by the macros which wrap it in a type-safe facade.
use super::argtypes::{
    wrap, CType, ProbeArgNativeType, ProbeArgNativeTypeInfo, ProbeArgType, ProbeArgWrapper,
};
use failure::Fallible;
use std::marker::PhantomData;

/// Each implementation of the tracing API provides a `Tracer` implementation, which provides
/// tracing functionality for an entire process.
pub trait Tracer: Drop + Sized {
    type ProviderBuilderType: ProviderBuilder<Self>;
    type ProviderType: Provider<Self>;
    type ProbeType: UnsafeProviderProbeImpl;

    fn new() -> Fallible<Self>;
    fn define_provider(
        &mut self,
        f: impl FnOnce(Self::ProviderBuilderType) -> Self::ProviderBuilderType,
    ) -> Fallible<&Self::ProviderType>;
}

pub trait ProviderBuilder<TracerT: Tracer> {
    fn add_probe(&self, definition: &ProbeDefinition) -> Fallible<()>;
    fn build(self) -> Fallible<<TracerT as Tracer>::ProviderType>;
}

pub trait Provider<TracerT: Tracer> {
    fn get_probe<ArgsT: ProbeArgs<ArgsT>>(
        &self,
        definition: &ProbeDefinition,
    ) -> Fallible<ProviderProbe<TracerT, ArgsT>> {
        let unsafe_impl = self.get_probe_unsafe(definition)?;
        Ok(ProviderProbe::new(unsafe_impl))
    }

    fn get_probe_unsafe(
        &self,
        definition: &ProbeDefinition,
    ) -> Fallible<<TracerT as Tracer>::ProbeType>;
}

pub struct ProviderProbe<TracerT: Tracer, ArgsT: ProbeArgs<ArgsT>> {
    unsafe_probe_impl: <TracerT as Tracer>::ProbeType,
    _args: PhantomData<ArgsT>,
}

impl<TracerT: Tracer, ArgsT: ProbeArgs<ArgsT>> ProviderProbe<TracerT, ArgsT> {
    fn new(probe: <TracerT as Tracer>::ProbeType) -> Self {
        ProviderProbe {
            unsafe_probe_impl: probe,
            _args: PhantomData,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.unsafe_probe_impl.is_enabled()
    }

    /// Fires the probe.  Note that it's assumed higher level code has tested `is_enable()` already.
    /// If the probe isnt' enabled, it's not an error to attempt to fire it, just a waste of cycles.
    pub fn fire(&self, args: ArgsT) -> () {
        args.fire_probe(&self.unsafe_probe_impl)
    }
}

/// All tuple types which consist entirely of elements which have a `ProbeArgType<T>` implementation
/// also implement `ProbeArgs<T>` where `T` is the tuple type.  This provides auto-generated methods
/// to obtain runtime information about the argument types, and also to wrap the elements and pass them
/// to the underlying C probing implementation in a typesafe manner.
pub trait ProbeArgs<T> {
    /// The tuple type corresponding to T where each element's type is the corresponding `ProbeArgWrapper`
    const ARG_COUNT: usize;

    /// A vector consisting of the CType enum corresponding to the C type which represents each element
    /// in the tuple.
    fn arg_types() -> Vec<CType>;

    /// Converts all of the probe args in this tuple to their C representations and passes them to the
    /// underlying UnsafeProviderProbeImpl implementation.
    fn fire_probe<ImplT: UnsafeProviderProbeImpl>(self, probe: &ImplT) -> ();
}

/// This structure is a runtime representation of a probe's definition.
/// A probe is defined by its name and the number and type of its arguments.  This is the
/// type used at runtime to identify probes and provide run-time type safety before allowing
/// probes to be co-erced into specific types.
#[derive(Clone, Debug, PartialEq)]
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

// The implementation of `ProbeArgs<T>` is provided for all tuples from
// 1to 12 elements.  That's highly repetitive, so there's code in `build.rs` that generates it.
// Here we need only include it:
include!(concat!(env!("OUT_DIR"), "/probe_args.rs"));
