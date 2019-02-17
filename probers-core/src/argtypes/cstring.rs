//! This module provides an implementation of the arg type plumbing which passes a rust `CString` to a native C function by simply passing
//! the pointer to the underlying memory.  This is inherently unsafe subject to the safety properties of the `CString` implementation.
use super::{ProbeArgType, ProbeArgWrapper};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

impl<'a> ProbeArgType<&'a CStr> for &'a CStr {
    type WrapperType = &'a CStr;
    fn wrap(arg: &'a CStr) -> Self::WrapperType {
        arg
    }
}

impl<'a> ProbeArgType<&'a CString> for &'a CString {
    type WrapperType = &'a CString;
    fn wrap(arg: &'a CString) -> Self::WrapperType {
        arg
    }
}

impl<'a> ProbeArgWrapper for &'a CStr {
    type CType = *const c_char;

    fn as_c_type(&self) -> Self::CType {
        self.as_ptr()
    }
}

impl<'a> ProbeArgWrapper for &'a CString {
    type CType = *const c_char;

    fn as_c_type(&self) -> Self::CType {
        self.as_ptr()
    }
}

#[cfg(test)]
mod tests {
    use crate::{wrap, ProbeArgWrapper};
    use quickcheck::TestResult;
    use std::ffi::{CStr, CString};

    #[quickcheck]
    fn cstring_to_cstring(x: String) -> TestResult {
        //quickcheck doesn't know how to generate CString values so we'll have to convert string
        //values.  Some of those won't work.
        match CString::new(x.as_str()) {
            Ok(cstring) => {
                let wrapper = wrap(&x);
                let pointer = wrapper.as_c_type();
                let cstr = unsafe { CStr::from_ptr(pointer) };

                assert_eq!(cstring.as_c_str(), cstr);
                TestResult::passed()
            }
            Err(_) => TestResult::discard(),
        }
    }
}
