use super::{ProbeArgType, ProbeArgWrapper};
use std::os::raw::*;

/// Wraps integer values of various types for conversion into the corresponding C types
#[derive(Debug)]
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
