//! This module implements `ProbeArgTraits` and `ProbeArgWrapper` for `Option<T>` for any `T`
//! whichwould itself be a valid probe argument type.  Well, sort of.  That's not entirely true.
//!
//! In fact `T` must be a `Copy` type, because in order to operate on the contents of the `Option` without consuming
//! it, we do so with references to `T`.  Since probe arguments are usually either string references or scalar types, this
//! restriction should not be a problem.
use super::{ProbeArgNativeType, ProbeArgTraits, ProbeArgType, ProbeArgWrapper};
use std::fmt::{Debug, Result};
use std::marker::Copy;

pub struct OptionWrapper<T>(Option<T>)
where
    T: ProbeArgWrapper;

impl<T> ProbeArgType<&Option<T>> for &Option<T>
where
    T: ProbeArgTraits<T> + Copy,
    for<'a> &'a T: ProbeArgTraits<T>,
{
    type WrapperType = OptionWrapper<<T as ProbeArgType<T>>::WrapperType>;

    fn wrap(arg: &Option<T>) -> Self::WrapperType {
        OptionWrapper(arg.as_ref().map(|x| super::wrap(x)))
    }
}

impl<T> ProbeArgWrapper for OptionWrapper<T>
where
    T: ProbeArgWrapper + Debug,
{
    type CType = <T as ProbeArgWrapper>::CType;

    fn as_c_type(&self) -> Self::CType {
        let wrapped = self.0.as_ref().map(|x| x.as_c_type());
        let default: Self::CType =
            <Self::CType as ProbeArgNativeType<Self::CType>>::get_default_value();

        wrapped.unwrap_or(default)
    }
}

impl<T> Debug for OptionWrapper<T>
where
    T: ProbeArgWrapper,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result {
        self.0.fmt(f)
    }
}
