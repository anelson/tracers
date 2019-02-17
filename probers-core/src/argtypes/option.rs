//! This module implements `ProbeArgTraits` and `ProbeArgWrapper` for `Option<T>` for any `T`
//! whichwould itself be a valid probe argument type.  Well, sort of.  That's not entirely true.
//!
//! In fact `T` must be a `Copy` type, because in order to operate on the contents of the `Option` without consuming
//! it, we do so with references to `T`.  Since probe arguments are usually either string references or scalar types, this
//! restriction should not be a problem.
use super::{ProbeArgNativeType, ProbeArgTraits, ProbeArgType, ProbeArgWrapper};
use std::ffi::CString;
use std::fmt::Debug;
use std::marker::Copy;

impl<T> ProbeArgType<&Option<T>> for &Option<T>
where
    T: ProbeArgTraits<T> + Copy,
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
        let wrapped = self.as_ref().map(|x| x.as_c_type());
        let default: Self::CType =
            <Self::CType as ProbeArgNativeType<Self::CType>>::get_default_value();

        wrapped.unwrap_or(default)
    }
}

#[cfg(test)]
mod tests {
    use crate::{wrap, ProbeArgWrapper};
    use std::ffi::{CStr, CString};
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

        let wrapper = wrap(&some);
        let pointer = wrapper.as_c_type();
        if pointer.is_null() {
            //This may happen if the string x has embedded NUL bytes.  In that case the string
            //cannot be represented as a C-style null terminated string.  Quicktest definitely
            //generates such strings so for testing purposes just confirm that is indeed what
            //happened
            assert!(CString::new(x.as_str()).ok().is_none());
        } else {
            //Behold this is very dangerous.  `pointer` is the address of the C string
            //which the wrapper created.  We'll use a `CStr` to attach to that pointer and then
            //convconert it into a Rust string type so we can perform the comparison of their
            //contents.  If `pointer` is not a valid pointer to a C-style string this can crash
            let cstr = unsafe { CStr::from_ptr(pointer) };
            let as_string = cstr
                .to_str()
                .expect("The string should always be valid unicode");

            assert_eq!(x, as_string)
        }

        assert_eq!(ptr::null(), wrap(&none).as_c_type());
        true
    }
}
