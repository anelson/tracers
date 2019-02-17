use super::{ProbeArgType, ProbeArgWrapper};
use std::ffi::CString;
use std::fmt::Debug;
use std::os::raw::c_char;

impl<'a> ProbeArgType<&'a str> for &'a str {
    type WrapperType = Option<CString>;
    fn wrap(arg: &'a str) -> Self::WrapperType {
        //Create a CString with the C representation of this string, which in theory can fail
        //if the Rust string has embedded NUL characters which the C string cannot represent
        CString::new(arg).ok()
    }
}

impl<'a> ProbeArgWrapper for Option<CString> {
    type CType = *const c_char;

    fn as_c_type(&self) -> Self::CType {
        self.as_ref()
            .map(|x| x.as_ptr())
            .unwrap_or(Self::default_c_value())
    }
}
