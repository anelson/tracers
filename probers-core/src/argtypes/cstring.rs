//! This module provides an implementation of the arg type plumbing which passes a rust `CString` to a native C function by simply passing
//! the pointer to the underlying memory.  This is inherently unsafe subject to the safety properties of the `CString` implementation.
use super::{ProbeArgType, ProbeArgWrapper};
use std::ffi::CString;
use std::fmt::Debug;
use std::os::raw::*;

pub struct CStringWrapper(CString);
pub struct CStringRefWrapper<'a>(&'a CString);

impl ProbeArgType<CString> for CString {
    type WrapperType = CStringWrapper;
}

impl<'a> ProbeArgType<&'a CString> for &'a CString {
    type WrapperType = CStringRefWrapper<'a>;
}

impl ProbeArgWrapper<CString> for CStringWrapper {
    type CType = *const c_char;

    fn new(arg: CString) -> Self {
        CStringWrapper(arg)
    }

    fn to_c_type(&mut self) -> Self::CType {
        self.0.as_ptr()
    }

    fn default_c_value() -> Self::CType {
        //In this case the default value is a null pointer.  Like it or not, it's a well-established convention
        //to use this value to indicate the absence of a real value
        0 as *const c_char
    }
}

impl<'a> ProbeArgWrapper<&'a CString> for CStringRefWrapper<'a> {
    type CType = *const c_char;

    fn new(arg: &'a CString) -> Self {
        CStringRefWrapper(arg)
    }

    fn to_c_type(&mut self) -> Self::CType {
        self.0.as_ptr()
    }

    fn default_c_value() -> Self::CType {
        //In this case the default value is a null pointer.  Like it or not, it's a well-established convention
        //to use this value to indicate the absence of a real value
        0 as *const c_char
    }
}

impl Debug for CStringWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        //Just use the Debug impl on the value returned by the function
        self.0.fmt(f)
    }
}

impl<'a> Debug for CStringRefWrapper<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        //Just use the Debug impl on the value returned by the function
        self.0.fmt(f)
    }
}
