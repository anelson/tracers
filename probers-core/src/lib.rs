use std::ffi::CString;
use std::os::raw::*;

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
pub trait ProbeArgWrapper<T> {
    type CType;

    fn new(arg: T) -> Self;

    /// Convert the probe argument from it's Rust type to one compatible with the native
    /// tracing library infrastructure.
    fn to_c_type(&mut self) -> Self::CType;

    /// This is ugly but unavoidable.  The underlying C type for an Opt<T> is the same C type as T.
    /// We will use the default value for T to indicate a value of None.  That will have to be good enough.
    fn default_c_value() -> Self::CType;
}

/// Wraps integer values of various types for conversion into the corresponding C types
pub struct IntWrapper<T>(T);

// Using the macro to avoid duplication, implement ProbeArgType<T> and ProbeArgWrapper<T> for
// the Rust integer types
macro_rules! impl_integer_arg_type {
    ( $rust_type:ident, $c_type:ident ) => {
        impl ProbeArgType<$rust_type> for $rust_type {
            type WrapperType = IntWrapper<$rust_type>;
        }

        impl ProbeArgWrapper<$rust_type> for IntWrapper<$rust_type> {
            type CType = $c_type;

            fn new(arg: $rust_type) -> Self {
                IntWrapper::<$rust_type>(arg)
            }

            fn to_c_type(&mut self) -> Self::CType {
                self.0 as $c_type
            }

            fn default_c_value() -> Self::CType {
                0 as $c_type
            }
        }
    };
}

impl_integer_arg_type!(u64, c_ulonglong);
impl_integer_arg_type!(i64, c_longlong);
impl_integer_arg_type!(u32, c_ulong);
impl_integer_arg_type!(i32, c_long);
impl_integer_arg_type!(u16, c_uint); //C variadics can't take shorts so these are passed as ints
impl_integer_arg_type!(i16, c_int);
impl_integer_arg_type!(u8, c_uint); //Ditto about chars
impl_integer_arg_type!(i8, c_int);

/// A wrapper around the Rust string slice type `&str`, which on demand converts the string slice
/// into a null-terminated C-style string.
pub struct StringWrapper<'a> {
    string: &'a str,
    c_string: Option<CString>,
}

impl<'a> ProbeArgType<&'a str> for &'a str {
    type WrapperType = StringWrapper<'a>;
}

impl<'a> ProbeArgWrapper<&'a str> for StringWrapper<'a> {
    type CType = *const c_char;

    fn new(arg: &'a str) -> Self {
        StringWrapper {
            string: arg,
            c_string: None,
        }
    }

    fn to_c_type(&mut self) -> Self::CType {
        // Create a CString from the string value, which can fail if there are embedded NUL bytes
        // in the string.  In that case we have a default string to fall back on
        match &self.c_string {
            None => match CString::new(self.string) {
                Ok(cstr) => {
                    //We are guaranteed that c_string will live as long as this struct does,
                    //so we can safely return a naked pointer
                    let c_ptr = cstr.as_ptr();
                    self.c_string = Some(cstr);
                    c_ptr
                }
                Err(_) => {
                    //This error means there was an embedded NUL byte in the string which can't be properly
                    //represented in a C string.  Use our default instead
                    //
                    //This probably looks like it's not safe but the c_str! macro gives us a string literal compiled into
                    //the binary so we can be guaranteed the memory never moves so this pointer is always valid
                    c_str_macro::c_str!("<probers-core failed to create CString>").as_ptr()
                }
            },
            Some(cstr) => {
                //If to_c_type is called twice that's a bug in the framework, but this code is needed
                //to guard against undefined behavior in that event.  If the c string has already been computed,
                //don't re-compute it as that means we have a dangling pointer returned from the first invocation of to_c_type
                cstr.as_ptr()
            }
        }
    }

    fn default_c_value() -> Self::CType {
        //In this case the default value is a null pointer.  Like it or not, it's a well-established convention
        //to use this value to indicate the absence of a real value
        0 as *const c_char
    }
}

pub struct OptionWrapper<T: ProbeArgType<T>> {
    inner_wrapper: Option<<T as ProbeArgType<T>>::WrapperType>,
}

impl<T> ProbeArgType<Option<T>> for Option<T>
where
    T: ProbeArgType<T>,
{
    type WrapperType = OptionWrapper<T>;
}

impl<T> ProbeArgWrapper<Option<T>> for OptionWrapper<T>
where
    T: ProbeArgType<T>,
{
    //When wrapping an Option<T>, the C type is the same as it would be for a T.
    //If there is no value for the Option<T>, we will use the default_c_value() value instead.
    type CType = <<T as ProbeArgType<T>>::WrapperType as ProbeArgWrapper<T>>::CType;

    fn new(arg: Option<T>) -> Self {
        let wrapped_arg =
            arg.map(|val| <<T as ProbeArgType<T>>::WrapperType as ProbeArgWrapper<T>>::new(val));
        OptionWrapper {
            inner_wrapper: wrapped_arg,
        }
    }

    fn to_c_type(&mut self) -> Self::CType {
        match &mut self.inner_wrapper {
            Some(wrapper) => wrapper.to_c_type(),
            None => Self::default_c_value(),
        }
    }

    fn default_c_value() -> Self::CType {
        <<T as ProbeArgType<T>>::WrapperType as ProbeArgWrapper<T>>::default_c_value()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
