//! This module provides an implementation of the arg type plumbing which passes a rust `CString` to a native C function by simply passing
//! the pointer to the underlying memory.  This is inherently unsafe subject to the safety properties of the `CString` implementation.
use super::{ProbeArgType, ProbeArgWrapper};
use std::ffi::CStr;
use std::os::raw::c_char;

impl<'a> ProbeArgType<&'a CStr> for &'a CStr {
    type WrapperType = &'a CStr;
    fn wrap(arg: &'a CStr) -> Self::WrapperType {
        arg
    }
}

impl<'a> ProbeArgWrapper for &'a CStr {
    type CType = *const c_char;

    fn as_c_type(&self) -> Self::CType {
        self.as_ptr()
    }
}
