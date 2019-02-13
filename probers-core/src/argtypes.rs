pub mod custom;
pub mod display;
pub mod int;
pub mod option;
pub mod string;

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

/// This trait, a companion to ProbeArgType<T>, wraps a supported type and on demand converts it to its equivalent C type.
/// For scalar types that are directly supported there is no overhead to this wrapping, but many more complicated types, including
/// Rust string types, need additional logic to produce a NULL-terminated byte array.
pub trait ProbeArgWrapper<T>: std::fmt::Debug {
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
pub fn wrap<T: ProbeArgType<T>>(arg: T) -> <T as ProbeArgType<T>>::WrapperType {
    <T as ProbeArgType<T>>::wrap(arg)
}
