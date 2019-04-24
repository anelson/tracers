//! This module and its submodules implement a type-safe wrapper around Rust types such that any
//! suitable type can be passed as a parameter to the C probing libraries with a minimum of
//! overhead.
//!
//! In order to be able to use a type as a probe argument, that type `T` must implement
//! `ProbeArgType<T>`, and there must be an implementation of `ProbeArgWrapper` suitable for that
//! type.
//!
//! This library provides implementations for all of the following:
//!
//! * All integer types from `u8/i8` to `u64/i64`
//! * `bool` (passed as an `i32` `1` means `true` and `0` means `false`)
//! * String references `&str`
//! * C-style string references `&CStr`
//! * `Option<T>` for any `T` which is itself a supported probe argument type and implements `Copy`
//! * Any pointer type, which is passed as either a 32- or 64-bit unsigned int depending upon
//! architecture
//!
//!
use std::fmt::Debug;
use strum_macros::IntoStaticStr;

pub mod bool;
pub mod cstring;
pub mod int;
pub mod native;
pub mod option;
pub mod pointer;
pub mod refs;
pub mod string;

pub use self::bool::*;
pub use cstring::*;
pub use int::*;
pub use native::*;
pub use option::*;
pub use pointer::*;
pub use refs::*;
pub use string::*;

#[derive(Debug, Clone, PartialEq, Hash, Eq, IntoStaticStr)]
pub enum CType {
    #[strum(serialize = "void")]
    NoArg,
    #[strum(serialize = "void*")]
    VoidPtr,
    #[strum(serialize = "char*")]
    CharPtr,
    #[strum(serialize = "unsigned char*")]
    UCharPtr,
    #[strum(serialize = "char")]
    Char,
    #[strum(serialize = "unsigned char")]
    UChar,
    #[strum(serialize = "short")]
    Short,
    #[strum(serialize = "unsigned short")]
    UShort,
    #[strum(serialize = "int")]
    Int,
    #[strum(serialize = "unsigned int")]
    UInt,
    #[strum(serialize = "long")]
    Long,
    #[strum(serialize = "unsigned long")]
    ULong,
    #[strum(serialize = "long long")]
    LongLong,
    #[strum(serialize = "unsigned long long")]
    ULongLong,
    #[strum(serialize = "size_t")]
    SizeT,
    #[strum(serialize = "ssize_t")]
    SSizeT,
}

/// Marker trait which decorates only those std::os::raw types which correspond to C types
/// supported by the C probing APIs.  Due to limitations of Rust's type system, this trait is split
/// into two parts: `ProbeArgNativeTypeInfo` which has no type parameter, and
/// `ProbeArgNativeType`which extends `ProbeArgNativeTypeInfo`, takes a type parameter `T` and
/// therefore adds the `get_default_value()` method.
pub trait ProbeArgNativeTypeInfo {
    fn get_c_type() -> CType;

    fn get_c_type_str() -> &'static str {
        // The #[strum...] attr contains the string representation of the C type for each member
        Self::get_c_type().into()
    }

    fn get_rust_type_str() -> &'static str;
}

/// The other half of `ProbeArgNativeTypeInfo`, which takes a type parameter and thus adds
/// `get_default_value`.
pub trait ProbeArgNativeType<T>: ProbeArgNativeTypeInfo {
    fn get_default_value() -> T;
}

/// This trait is defined on any type which is supported as an argument to a probe.
///
/// In general only scalar integer types are supported directly, though pointers can be passed
/// as u64 values and the tracing code is then responsible for knowing what to do with the pointer
/// (for example, treat it as a null-terminated UTF-8 string, or a pointer to a certain structure, etc).
pub trait ProbeArgType<T> {
    type WrapperType: ProbeArgWrapper;

    fn wrap(arg: T) -> Self::WrapperType;
}

/// This trait, a companion to ProbeArgType<T>, wraps a supported type and on demand converts it to its equivalent C type.
/// For scalar types that are directly supported there is no overhead to this wrapping, but many more complicated types, including
/// Rust string types, need additional logic to produce a NULL-terminated byte array.
pub trait ProbeArgWrapper: Debug
where
    //How's this for a type restriction: it says the CType must be one we've marked as a native
    //type
    <Self as ProbeArgWrapper>::CType: ProbeArgNativeType<<Self as ProbeArgWrapper>::CType>,
{
    type CType: ProbeArgNativeTypeInfo;

    /// Convert the probe argument from it's Rust type to one compatible with the native
    /// tracing library infrastructure.
    fn as_c_type(&self) -> Self::CType;

    /// This is ugly but unavoidable.  The underlying C type for an Opt<T> is the same C type as T.
    /// We will use the default value for T to indicate a value of None.  That will have to be good enough.
    fn default_c_value() -> Self::CType {
        <Self::CType as ProbeArgNativeType<Self::CType>>::get_default_value()
    }
}

/// Helper function to wrap a probe arg in its correspondong wrapper without contorting one's fingers typing angle brackets
pub fn wrap<T: ProbeArgType<T>>(arg: T) -> <T as ProbeArgType<T>>::WrapperType {
    <T as ProbeArgType<T>>::wrap(arg)
}
