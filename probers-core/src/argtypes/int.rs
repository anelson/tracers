use super::{ProbeArgType, ProbeArgWrapper};
use std::os::raw;

#[cfg(test)]
extern crate quickcheck;

// Using the macro to avoid duplication, implement ProbeArgType and ProbeArgWrapper for the
// intrinsic integer types, with a few non-obvious translations based on C variadic idiosyncracies
macro_rules! impl_integer_arg_type {
    ( $rust_type:ty, $c_type:ty, $tests:ident ) => {
        impl ProbeArgType<$rust_type> for $rust_type {
            type WrapperType = $rust_type;

            fn wrap(arg: $rust_type) -> Self::WrapperType {
                arg
            }
        }

        impl ProbeArgWrapper for $rust_type {
            type CType = $c_type;

            fn as_c_type(&self) -> Self::CType {
                //*self as $c_type
                <$c_type>::from(*self)
            }
        }

        #[cfg(test)]
        mod $tests {
            use crate::{wrap, ProbeArgWrapper};
            use std::mem::size_of;

            #[quickcheck]
            fn converts_to_c_type(x: $rust_type) {
                let wrapper = wrap(x);

                assert_eq!(size_of!($rust_type), size_of!($c_type));
                assert_eq!(<$c_type>::from(x), wrapper.as_c_type());
            }
        }
    };
}

impl_integer_arg_type!(usize, libc::size_t, usize_test);
impl_integer_arg_type!(isize, libc::ssize_t, isize_test);
impl_integer_arg_type!(u64, raw::c_ulonglong, u64_test);
impl_integer_arg_type!(i64, raw::c_longlong, i64_test);
impl_integer_arg_type!(u32, raw::c_uint, u32_test);
impl_integer_arg_type!(i32, raw::c_int, i32_test);
impl_integer_arg_type!(u16, raw::c_ushort, u16_test);
impl_integer_arg_type!(i16, raw::c_short, i16_test);
impl_integer_arg_type!(u8, raw::c_uchar, u8_test);
impl_integer_arg_type!(i8, raw::c_char, i8_test);
