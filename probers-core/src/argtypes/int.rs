use super::{ProbeArgType, ProbeArgWrapper};

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

            #[quickcheck]
            fn converts_to_c_type(x: $rust_type) {
                let wrapper = wrap(x);

                assert_eq!(<$c_type>::from(x), wrapper.as_c_type());
            }
        }
    };
}

impl_integer_arg_type!(usize, usize, usize_test);
impl_integer_arg_type!(u64, u64, u64_test);
impl_integer_arg_type!(i64, i64, i64_test);
impl_integer_arg_type!(u32, u32, u32_test);
impl_integer_arg_type!(i32, i32, i32_test);
impl_integer_arg_type!(u16, u32, u16_test); //C variadics can't take shorts so these are passed as ints
impl_integer_arg_type!(i16, i32, i16_test);
impl_integer_arg_type!(u8, u32, u8_test); //Ditto about chars
impl_integer_arg_type!(i8, i32, i8_test);
