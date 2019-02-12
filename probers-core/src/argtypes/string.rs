use super::{ProbeArgType, ProbeArgWrapper};
use std::ffi::CString;
use std::os::raw::*;

/// A wrapper around the Rust string slice type `&str`, which on demand converts the string slice
/// into a null-terminated C-style string.
#[derive(Debug)]
pub struct StringWrapper<'a> {
    string: &'a str,
    c_string: Option<CString>,
}

impl<'a> ProbeArgType<&'a str> for &'a str {
    type WrapperType = StringWrapper<'a>;
}

impl<'a> ProbeArgWrapper<&'a str> for StringWrapper<'a> {
    type CType = *const c_char;

    fn new(arg: &'a str) -> Self {
        StringWrapper {
            string: arg,
            c_string: None,
        }
    }

    fn to_c_type(&mut self) -> Self::CType {
        // Create a CString from the string value, which can fail if there are embedded NUL bytes
        // in the string.  In that case we have a default string to fall back on
        match &self.c_string {
            None => match CString::new(self.string) {
                Ok(cstr) => {
                    //We are guaranteed that c_string will live as long as this struct does,
                    //so we can safely return a naked pointer
                    let c_ptr = cstr.as_ptr();
                    self.c_string = Some(cstr);
                    c_ptr
                }
                Err(_) => {
                    //This error means there was an embedded NUL byte in the string which can't be properly
                    //represented in a C string.  Use our default instead
                    //
                    //This probably looks like it's not safe but the c_str! macro gives us a string literal compiled into
                    //the binary so we can be guaranteed the memory never moves so this pointer is always valid
                    c_str_macro::c_str!("<probers-core failed to create CString>").as_ptr()
                }
            },
            Some(cstr) => {
                //If to_c_type is called twice that's a bug in the framework, but this code is needed
                //to guard against undefined behavior in that event.  If the c string has already been computed,
                //don't re-compute it as that means we have a dangling pointer returned from the first invocation of to_c_type
                cstr.as_ptr()
            }
        }
    }

    fn default_c_value() -> Self::CType {
        //In this case the default value is a null pointer.  Like it or not, it's a well-established convention
        //to use this value to indicate the absence of a real value
        0 as *const c_char
    }
}
