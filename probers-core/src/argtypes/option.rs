//! This module implements `ProbeArgType` and `ProbeArgWrapper` for `Option<T>` for any `T`
//! whichwould itself be a valid probe argument type.  Well, sort of.  That's not entirely true.
//!
//! In fact `T` must be a `Copy` type, because in order to operate on the contents of the `Option` without consuming
//! it, we do so with references to `T`.  Since probe arguments are usually either string references or scalar types, this
//! restriction should not be a problem.
use super::{ProbeArgNativeType, ProbeArgType, ProbeArgWrapper};
use std::ffi::CString;
use std::fmt::Debug;
use std::marker::Copy;

impl<T> ProbeArgType<&Option<T>> for &Option<T>
where
    T: ProbeArgType<T> + Copy,
{
    type WrapperType = Option<<T as ProbeArgType<T>>::WrapperType>;

    fn wrap(arg: &Option<T>) -> Self::WrapperType {
        arg.as_ref().map(super::wrap)
    }
}

/// The general implementation for `Option<T>` won't work for `Option<String>` because we don't
/// support the `String` type, only references like `&String` or `&str`.  But an `Option<String>`
/// is quite often used and it should be supported, so we'll implement it directly here.
///
/// The result is the same as the outcome of the conversion in the `string` module.
impl ProbeArgType<&Option<String>> for &Option<String> {
    type WrapperType = Option<CString>;

    fn wrap(arg: &Option<String>) -> Self::WrapperType {
        arg.as_ref().and_then(super::wrap)
    }
}

impl<T> ProbeArgWrapper for Option<T>
where
    T: ProbeArgWrapper + Debug,
{
    type CType = <T as ProbeArgWrapper>::CType;

    fn as_c_type(&self) -> Self::CType {
        let wrapped = self.as_ref().map(ProbeArgWrapper::as_c_type);
        let default: Self::CType =
            <Self::CType as ProbeArgNativeType<Self::CType>>::get_default_value();

        wrapped.unwrap_or(default)
    }
}

#[cfg(test)]
mod tests {
    use crate::{wrap, ProbeArgWrapper};
    use std::ptr;

    #[quickcheck]
    fn option_ints(x: i64) -> bool {
        let some = Some(x);
        let none: Option<i64> = None;

        assert_eq!(x, wrap(&some).as_c_type());
        assert_eq!(0, wrap(&none).as_c_type());
        true
    }

    #[quickcheck]
    fn option_strings(x: String) -> bool {
        let some = Some(x.clone());
        let none: Option<String> = None;
        let naked_wrapper = wrap(&x);
        let opt_wrapper = wrap(&some);

        assert_eq!(naked_wrapper, opt_wrapper);
        assert_eq!(ptr::null(), wrap(&none).as_c_type());

        // the same results should be produced for string references, except
        // because those are handled by the generalized Option implementation and not the one
        // specifically for Option<String>, there is an unfortunate double layer of Option, because
        // the internal wrapper type for a &str is itself an Option<CString>, so an Option<&str>
        // has a wrapper type of Option<Option<CString>>.  Ugly but without support for partial
        // specialization in Rust I don't see a way around it.
        let some = Some(x.as_str());
        let none = None;
        let naked_wrapper = wrap(x.as_str());
        let opt_wrapper = wrap(&some);

        assert_eq!(Some(naked_wrapper), opt_wrapper);
        assert_eq!(ptr::null(), wrap::<&Option<&str>>(&none).as_c_type());

        true
    }
}
