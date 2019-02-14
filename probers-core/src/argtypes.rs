use std::fmt::Debug;

pub mod cstring;
pub mod func;
pub mod int;
pub mod option;
pub mod reftype;
pub mod string;

pub use cstring::*;
pub use func::*;
pub use int::*;
pub use option::*;
pub use reftype::*;
pub use string::*;

/// This trait is defined on any type for which ProbeArgType<T> is defined.  It's a workaround
/// Rust's limitations on implementing foreign traits.  We need to be assured we can get a string
/// representation of any type for the implementation of tracing that uses Rust logging, and
/// `Debug` is very convenient to implement while `Display` is not, so we use Debug as the basis.
///
/// There is a default implementation of this trait for all types that implement `Debug`.  Users
/// of this library should not find themselves having to implement this trait; instead ensure
/// your times implement `Debug` and this implementation will exist automatically.
pub trait ProbeArgDebug<T>: std::fmt::Debug {}

impl<T> ProbeArgDebug<T> for T where T: Debug {}

/// This trait is defined on any type which is supported as an argument to a probe.
///
/// In general only scalar integer types are supported directly, though pointers can be passed
/// as u64 values and the tracing code is then responsible for knowing what to do with the pointer
/// (for example, treat it as a null-terminated UTF-8 string, or a pointer to a certain structure, etc).
pub trait ProbeArgType<T> {
    type WrapperType: ProbeArgWrapper<T>;

    fn wrap(arg: T) -> Self::WrapperType {
        Self::WrapperType::new(arg)
    }
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
pub trait ProbeArgTraits<T>: ProbeArgType<T> + ProbeArgDebug<T> {}

impl<T> ProbeArgTraits<T> for T where T: ProbeArgType<T> + ProbeArgDebug<T> {}

/// This trait, a companion to ProbeArgType<T>, wraps a supported type and on demand converts it to its equivalent C type.
/// For scalar types that are directly supported there is no overhead to this wrapping, but many more complicated types, including
/// Rust string types, need additional logic to produce a NULL-terminated byte array.
pub trait ProbeArgWrapper<T>: Debug {
    type CType;

    fn new(arg: T) -> Self;

    /// Convert the probe argument from it's Rust type to one compatible with the native
    /// tracing library infrastructure.
    fn to_c_type(&mut self) -> Self::CType;

    /// This is ugly but unavoidable.  The underlying C type for an Opt<T> is the same C type as T.
    /// We will use the default value for T to indicate a value of None.  That will have to be good enough.
    fn default_c_value() -> Self::CType;
}

/// Helper function to wrap a probe arg in its correspondong wrapper without contorting one's fingers typing angle brackets
pub fn wrap<T: ProbeArgTraits<T>>(arg: T) -> <T as ProbeArgType<T>>::WrapperType {
    <T as ProbeArgType<T>>::wrap(arg)
}
