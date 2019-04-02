//! This module implements ProbeArgType and ProbeArgWrapper for Rust's string types.  This
//! implementation will be available for `&String` and `&str` on all supported platforms.  On the
//! `unix` family of platforms, `&OsString` and `&OsStr` are also supported.
//!
//! In all four of these cases the idea is the same.  The wrapper for all of the string types is
//! `Option<CString>`, which will contain either nothing or a `CString` containing the C
//! representation (meaning null terminated).  Since Rust strings can contain embedded NULL
//! bytes,this means that some Rust strings cannot be represented as CStrings.  Hence the use of
//! `Option`.  If the string can't be represented as a `CString`, it will be passed to the C
//! probeAPI as a NULL.
use super::{ProbeArgType, ProbeArgWrapper};
#[cfg(unix)]
use std::ffi::{CString, OsStr, OsString};
use std::os::raw::c_char;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

impl ProbeArgType<&str> for &str {
    type WrapperType = Option<CString>;
    fn wrap(arg: &str) -> Self::WrapperType {
        CString::new(arg).ok()
    }
}

impl ProbeArgType<&String> for &String {
    type WrapperType = Option<CString>;
    fn wrap(arg: &String) -> Self::WrapperType {
        CString::new(arg.as_str()).ok()
    }
}

#[cfg(unix)] // Only the unix impl of OsStr/OsString exposes the string as bytes
impl ProbeArgType<&OsStr> for &OsStr {
    type WrapperType = Option<CString>;
    fn wrap(arg: &OsStr) -> Self::WrapperType {
        CString::new(arg.as_bytes()).ok()
    }
}

#[cfg(unix)] // Only the unix impl of OsStr/OsString exposes the string as bytes
impl ProbeArgType<&OsString> for &OsString {
    type WrapperType = Option<CString>;
    fn wrap(arg: &OsString) -> Self::WrapperType {
        CString::new(arg.as_bytes()).ok()
    }
}

impl<'a> ProbeArgWrapper for Option<CString> {
    type CType = *const c_char;

    fn as_c_type(&self) -> Self::CType {
        self.as_ref()
            .map(|x| x.as_ptr())
            .unwrap_or_else(Self::default_c_value)
    }
}

/// The tests module is a bit messy because it is exercising four slightly different implementations:
/// * `&String`
/// * `&str`
/// * `&OsString`
/// * `&OsStr`
#[cfg(test)]
mod tests {
    use crate::{wrap, ProbeArgType, ProbeArgWrapper};
    use std::ffi::{CStr, CString};
    use std::ffi::{OsStr, OsString};
    use std::fmt::Debug;
    #[cfg(unix)]
    use std::os::unix::ffi::OsStrExt;

    #[quickcheck]
    fn string_as_c_type(x: String) -> bool {
        let wrapper = wrap(&x);
        let pointer: *const i8 = wrapper.as_c_type();
        test_with_string(&x, pointer);
        true
    }

    #[quickcheck]
    fn str_as_c_type(x: String) -> bool {
        let wrapper = wrap(x.as_str());
        let pointer: *const i8 = wrapper.as_c_type();
        test_with_string(x.as_str(), pointer);
        true
    }

    #[quickcheck]
    #[cfg(unix)]
    fn osstring_as_c_type(x: OsString) -> bool {
        let wrapper = wrap(&x);
        let pointer: *const i8 = wrapper.as_c_type();
        test_with_string(&x, pointer);
        true
    }

    #[quickcheck]
    #[cfg(unix)]
    fn osstr_as_c_type(x: OsString) -> bool {
        let wrapper = wrap(x.as_os_str());
        let pointer: *const i8 = wrapper.as_c_type();
        test_with_string(x.as_os_str(), pointer);
        true
    }

    /// Implementing this trait for each of the supported string types helps take out some of the
    /// repetition in the test code
    trait StringHelpers {
        fn to_cstring(&self) -> Option<CString>;
        fn assert_equals(&self, x: &str) -> ();
    }

    impl StringHelpers for &str {
        fn to_cstring(&self) -> Option<CString> {
            CString::new(*self).ok()
        }
        fn assert_equals(&self, x: &str) -> () {
            assert_eq!(*self, x)
        }
    }

    impl StringHelpers for &String {
        fn to_cstring(&self) -> Option<CString> {
            CString::new(self.as_str()).ok()
        }
        fn assert_equals(&self, x: &str) -> () {
            assert_eq!(self.as_str(), x)
        }
    }

    #[cfg(unix)]
    impl StringHelpers for &OsStr {
        fn to_cstring(&self) -> Option<CString> {
            CString::new(Vec::from(self.as_bytes())).ok()
        }

        fn assert_equals(&self, x: &str) -> () {
            assert_eq!(self.to_str().expect("should always be valid UTF-8"), x)
        }
    }

    #[cfg(unix)]
    impl StringHelpers for &OsString {
        fn to_cstring(&self) -> Option<CString> {
            CString::new(Vec::from(self.as_bytes())).ok()
        }
        fn assert_equals(&self, x: &str) -> () {
            assert_eq!(self.to_str().expect("should always be valid UTF-8"), x)
        }
    }

    fn test_with_string<'a, T: StringHelpers + ProbeArgType<T> + Debug>(x: T, pointer: *const i8) {
        if pointer.is_null() {
            //This may happen if the string x has embedded NUL bytes.  In that case the string
            //cannot be represented as a C-style null terminated string.  Quicktest definitely
            //generates such strings so for testing purposes just confirm that is indeed what
            //happened
            assert!(x.to_cstring().is_none());
        } else {
            //Behold this is very dangerous.  `pointer` is the address of the C string
            //which the wrapper created.  We'll use a `CStr` to attach to that pointer and then
            //convconert it into a Rust string type so we can perform the comparison of their
            //contents.  If `pointer` is not a valid pointer to a C-style string this can crash
            let cstr = unsafe { CStr::from_ptr(pointer) };
            let as_string = cstr
                .to_str()
                .expect("The string should always be valid unicode");

            x.assert_equals(as_string);
        }
    }
}
