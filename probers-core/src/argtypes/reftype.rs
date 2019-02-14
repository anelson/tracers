//! This module contains a generic implementation of `ProbeArgWrapper<T>` for cases in which `T`
//! is some type, presumably not a primitive type, which is passed by reference and which cannot
//! be directly translated into a C type.  Examples of this are Rust `str` and any Rust `struct`.
//!
//! This wrapper requires only that a `RefTypeConverter` trait be implemented to define how to
//! map from the reference type `RefT` to the desired C-compatible primitive type `PrimitiveT`.
//! Given an implementation of this trait, the mechanics of implementing `ProbeArgWrapper<RefT>`
//! are provided in this module in the form of the `RefTypeWrapper` struct and corresponding
//! trait implementations.
//!
//! This provides the basis for two other modules, `func` and `string`, which provide support for
//! arbitrary Rust `Fn` and `str`, respectively.
use super::{ProbeArgDebug, ProbeArgTraits, ProbeArgType, ProbeArgWrapper};
use std::fmt::Debug;

pub trait RefTypeConverter<RefT, PrimitiveT>
where
    RefT: ?Sized,
    PrimitiveT: ProbeArgTraits<PrimitiveT>,
{
    fn ref_to_primitive(arg: &RefT) -> Option<PrimitiveT>;
}

/// Generic wrapper around any type which is stored as a non-static reference, and which can be
/// converted into some other type `T` for which a `ProbeArgType<T>` implementation exists.
///
/// The conversion from `RefT` to `PrimitiveT` is performed lazily, such that if a particular probe
/// is not enabled, no conversion will be done.
pub struct RefTypeWrapper<'a, RefT, PrimitiveT, ConverterT>(
    &'a RefT,
    Option<<PrimitiveT as ProbeArgType<PrimitiveT>>::WrapperType>,
    std::marker::PhantomData<ConverterT>,
)
where
    RefT: ?Sized,
    &'a RefT: ProbeArgDebug<&'a RefT>,
    ConverterT: RefTypeConverter<RefT, PrimitiveT>,
    PrimitiveT: ProbeArgTraits<PrimitiveT>;

impl<'a, RefT, PrimitiveT, ConverterT> RefTypeWrapper<'a, RefT, PrimitiveT, ConverterT>
where
    RefT: ?Sized,
    &'a RefT: ProbeArgDebug<&'a RefT>,
    ConverterT: RefTypeConverter<RefT, PrimitiveT>,
    PrimitiveT: ProbeArgTraits<PrimitiveT>,
{
    /// Invokes a closure passing a mutable reference to the generated wrapper for the primitive
    /// type constructed from the reference type.  If the wrapper was previously created this
    /// uses the previously created wrapper, if not it creates a new one first.
    fn with_primitive_wrapper<F, ReturnT>(&mut self, f: F) -> Option<ReturnT>
    where
        F: FnOnce(&mut <PrimitiveT as ProbeArgType<PrimitiveT>>::WrapperType) -> ReturnT,
    {
        //It's not enough to just call the underlying wrapper's to_c_type().  In some cases
        //(specifically with the StringWrapper for Rust strings), memory is allocated which is
        //freed when the wrapper goes out of scope.  Thus we have to keep it around, hence the
        //Option type.
        let ref_arg = self.0;
        if self.1.is_none() {
            let primitive_arg: Option<PrimitiveT> = ConverterT::ref_to_primitive(ref_arg);
            let wrapped_arg = primitive_arg.map(super::wrap);
            self.1 = wrapped_arg;
        }

        self.1.as_mut().map(f)
    }
}

impl<'a, RefT, PrimitiveT, ConverterT> ProbeArgWrapper<&'a RefT>
    for RefTypeWrapper<'a, RefT, PrimitiveT, ConverterT>
where
    RefT: ?Sized,
    &'a RefT: ProbeArgDebug<&'a RefT>,
    PrimitiveT: ProbeArgTraits<PrimitiveT>,
    ConverterT: RefTypeConverter<RefT, PrimitiveT>,
{
    type CType = <<PrimitiveT as ProbeArgType<PrimitiveT>>::WrapperType as ProbeArgWrapper<
        PrimitiveT,
    >>::CType;

    fn new(arg: &'a RefT) -> Self {
        RefTypeWrapper(arg, None, std::marker::PhantomData::default())
    }

    fn to_c_type(&mut self) -> Self::CType {
        self.with_primitive_wrapper(|wrapper| wrapper.to_c_type())
            .unwrap_or(Self::default_c_value())
    }

    fn default_c_value() -> Self::CType {
        // And you thought C++ templates made for some gnarly type names...
        <<PrimitiveT as ProbeArgType<PrimitiveT>>::WrapperType as ProbeArgWrapper<
        PrimitiveT,
        >>::default_c_value()
    }
}

impl<'a, RefT, PrimitiveT, ConverterT> Debug for RefTypeWrapper<'a, RefT, PrimitiveT, ConverterT>
where
    RefT: ?Sized,
    &'a RefT: ProbeArgDebug<&'a RefT>,
    PrimitiveT: ProbeArgTraits<PrimitiveT>,
    ConverterT: RefTypeConverter<RefT, PrimitiveT>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        //Just use the Debug impl on the value returned by the function
        self.0.fmt(f)
    }
}
