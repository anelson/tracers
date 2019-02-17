//! This module implements the marker trait `ProbeArgNativeType` for those Rust types that correspond directly to C types, and thus are the ultimate
//! result types for any transformation of a Rust type into a value suitable for passing to the C tracing API.
use super::{CType, ProbeArgNativeType, ProbeArgNativeTypeInfo};
use std::os::raw::*;

// Using the macro to avoid duplication
macro_rules! impl_native_type_trait {
    ( $rust_type:ty, $c_type:expr, $default_value:expr ) => {
        impl ProbeArgNativeTypeInfo for $rust_type {
            fn get_c_type() -> CType {
                $c_type
            }
        }

        impl ProbeArgNativeType<$rust_type> for $rust_type {
            fn get_default_value() -> $rust_type {
                $default_value as $rust_type
            }
        }
    };
}

impl_native_type_trait!(u64, CType::ULongLong, 0);
impl_native_type_trait!(i64, CType::LongLong, 0);
impl_native_type_trait!(u32, CType::UInt, 0);
impl_native_type_trait!(i32, CType::Int, 0);
impl_native_type_trait!(u16, CType::UShort, 0);
impl_native_type_trait!(i16, CType::Short, 0);
impl_native_type_trait!(u8, CType::UChar, 0);
impl_native_type_trait!(i8, CType::Char, 0);
impl_native_type_trait!(*const c_void, CType::VoidPtr, 0);
impl_native_type_trait!(*const c_char, CType::CharPtr, 0);
