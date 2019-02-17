use super::{ProbeArgType, ProbeArgWrapper};

// Using the macro to avoid duplication, implement ProbeArgType and ProbeArgWrapper for the
// intrinsic integer types, with a few non-obvious translations based on C variadic idiosyncracies
macro_rules! impl_integer_arg_type {
    ( $rust_type:ty, $c_type:ty ) => {
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
    };
}

impl_integer_arg_type!(u64, u64);
impl_integer_arg_type!(i64, i64);
impl_integer_arg_type!(u32, u32);
impl_integer_arg_type!(i32, i32);
impl_integer_arg_type!(u16, u32); //C variadics can't take shorts so these are passed as ints
impl_integer_arg_type!(i16, i32);
impl_integer_arg_type!(u8, u8); //Ditto about chars
impl_integer_arg_type!(i8, i8);
