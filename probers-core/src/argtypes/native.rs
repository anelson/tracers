//! This module implements the marker trait `ProbeArgNativeType` for those Rust types that correspond directly to C types, and thus are the ultimate
//! result types for any transformation of a Rust type into a value suitable for passing to the C tracing API.
use super::{CType, ProbeArgNativeType, ProbeArgNativeTypeInfo};
use std::os::raw::*;
use std::ptr;

// Using the macro to avoid duplication
macro_rules! impl_native_type_trait_and_default {
    ( $rust_type:ty, $c_type:expr, $default:expr ) => {
        impl ProbeArgNativeTypeInfo for $rust_type {
            fn get_c_type() -> CType {
                $c_type
            }
        }

        impl ProbeArgNativeType<$rust_type> for $rust_type {
            fn get_default_value() -> $rust_type {
                $default
            }
        }
    };
}

macro_rules! impl_native_type_trait {
    ( $rust_type:ty, $c_type:expr ) => {
        impl_native_type_trait_and_default!($rust_type, $c_type, Default::default());
    };
}

#[cfg(target_pointer_width = "16")]
impl_native_type_trait!(usize, CType::UShort);
#[cfg(target_pointer_width = "32")]
impl_native_type_trait!(usize, CType::UInt);
#[cfg(target_pointer_width = "64")]
impl_native_type_trait!(usize, CType::ULongLong);

impl_native_type_trait!(u64, CType::ULongLong);
impl_native_type_trait!(i64, CType::LongLong);
impl_native_type_trait!(u32, CType::UInt);
impl_native_type_trait!(i32, CType::Int);
impl_native_type_trait!(u16, CType::UShort);
impl_native_type_trait!(i16, CType::Short);
impl_native_type_trait!(u8, CType::UChar);
impl_native_type_trait!(i8, CType::Char);
impl_native_type_trait_and_default!(*const c_void, CType::VoidPtr, ptr::null());
impl_native_type_trait_and_default!(*const c_char, CType::CharPtr, ptr::null());
