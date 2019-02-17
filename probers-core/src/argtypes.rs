use std::fmt::{Debug, Result};

pub mod cstring;
pub mod int;
pub mod native;
pub mod option;
pub mod refs;
//pub mod reftype;
pub mod string;

pub use cstring::*;
pub use int::*;
pub use native::*;
pub use option::*;
pub use refs::*;
//pub use reftype::*;
pub use string::*;

pub enum CType {
    NoArg,
    VoidPtr,
    CharPtr,
    Char,
    UChar,
    Short,
    UShort,
    Int,
    UInt,
    Long,
    ULong,
    LongLong,
    ULongLong,
}

/// Marker trait which decorates only those std::os::raw types which correspond to C types
/// supported by the C probing APIs.  Due to limitations of Rust's type system, this trait is split
/// into two parts: `ProbeArgNativeTypeInfo` which has no type parameter, and
/// `ProbeArgNativeType`which extends `ProbeArgNativeTypeInfo`, takes a type parameter `T` and
/// therefore adds the `get_default_value()` method.
pub trait ProbeArgNativeTypeInfo {
    fn get_c_type() -> CType;
}

/// The other half of `ProbeArgNativeTypeInfo`, which takes a type parameter and thus adds
/// `get_default_value`.
pub trait ProbeArgNativeType<T>: ProbeArgNativeTypeInfo {
    fn get_default_value() -> T;
}

/// This trait is defined on any type for which ProbeArgType<T> is defined.  It's a workaround
/// Rust's limitations on implementing foreign traits.  We need to be assured we can get a string
/// representation of any type for the implementation of tracing that uses Rust logging, and
/// `Debug` is very convenient to implement while `Display` is not, so we use Debug as the basis.
///
/// There is a default implementation of this trait for all types that implement `Debug`.  Users
/// of this library should not find themselves having to implement this trait; instead ensure
/// your times implement `Debug` and this implementation will exist automatically.
pub trait ProbeArgDebug<T> {
    fn debug_format(&self, f: &mut std::fmt::Formatter) -> Result;
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

/// This trait helps us keep the code readable in spite of Rust's limitation in which type
/// restrictions on super traits are not propagated intelligently.  This allows us to write type
/// constraints like:
/// ```noexecute
/// ... where T: ProbeArgTraits<T>
/// ```
///
/// instead of:
///
/// ```noexecute
/// ... where T: ProbeArgDebug<T> + ProbeArgType<T> + (whatever we might add)
/// ```
pub trait ProbeArgTraits<T>: ProbeArgType<T> {}

impl<T> ProbeArgTraits<T> for T where T: ProbeArgType<T> {}

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
pub fn wrap<T: ProbeArgTraits<T>>(arg: T) -> <T as ProbeArgType<T>>::WrapperType {
    <T as ProbeArgType<T>>::wrap(arg)
}
