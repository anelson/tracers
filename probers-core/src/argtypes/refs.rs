//! This module ensures that if `ProbeArgTraits` and `ProbeArgWrapper` are implemented for some `T`, then they are also implemented for `&T`.
//! This is necessary for convenience and also to support our generialized implementation for `Option<T>`

use super::{ProbeArgNativeType, ProbeArgTraits, ProbeArgType, ProbeArgWrapper};
use std::fmt::Debug;
use std::marker::Copy;

impl<'a, T> ProbeArgType<&'a T> for &'a T
where
    T: ProbeArgTraits<T> + Copy,
{
    type WrapperType = <T as ProbeArgType<T>>::WrapperType;

    fn wrap(arg: &'a T) -> Self::WrapperType {
        super::wrap(*arg)
    }
}
