//! This module implements the marker trait `ProbeArgNativeType` for those Rust types that correspond directly to C types, and thus are the ultimate
//! result types for any transformation of a Rust type into a value suitable for passing to the C tracing API.
use super::{CType, ProbeArgNativeType, ProbeArgNativeTypeInfo};
use std::ptr;

// Using the macro to avoid duplication
macro_rules! impl_native_type_trait_and_default {
    ( $rust_type:ty, $c_type:expr, $default:expr ) => {
        impl ProbeArgNativeTypeInfo for $rust_type {
            fn get_c_type() -> CType {
                $c_type
            }

            fn get_rust_type_str() -> &'static str {
                stringify!($rust_type)
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

impl_native_type_trait!(libc::size_t, CType::SizeT);
impl_native_type_trait!(libc::ssize_t, CType::SSizeT);

impl_native_type_trait!(std::os::raw::c_ulonglong, CType::ULongLong);
impl_native_type_trait!(std::os::raw::c_longlong, CType::LongLong);
impl_native_type_trait!(std::os::raw::c_uint, CType::UInt);
impl_native_type_trait!(std::os::raw::c_int, CType::Int);
impl_native_type_trait!(std::os::raw::c_ushort, CType::UShort);
impl_native_type_trait!(std::os::raw::c_short, CType::Short);
impl_native_type_trait!(std::os::raw::c_uchar, CType::UChar);
impl_native_type_trait!(std::os::raw::c_char, CType::Char);
impl_native_type_trait_and_default!(*const std::os::raw::c_void, CType::VoidPtr, ptr::null());
impl_native_type_trait_and_default!(*const std::os::raw::c_char, CType::CharPtr, ptr::null());
impl_native_type_trait_and_default!(*const std::os::raw::c_uchar, CType::UCharPtr, ptr::null());
