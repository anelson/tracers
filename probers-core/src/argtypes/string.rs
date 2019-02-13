use super::{ProbeArgType, ProbeArgWrapper};
use std::ffi::CString;
use std::os::raw::*;

/// A wrapper around the Rust string slice type `&str`, which on demand converts the string slice
/// into a null-terminated C-style string.
#[derive(Debug)]
pub struct StringWrapper<'a>(&'a str, Option<CString>);

impl<'a> ProbeArgType<&'a str> for &'a str {
    type WrapperType = StringWrapper<'a>;
}

impl<'a> ProbeArgWrapper<&'a str> for StringWrapper<'a> {
    type CType = *const c_char;

    fn new(arg: &'a str) -> Self {
        StringWrapper(arg, None)
    }

    fn to_c_type(&mut self) -> Self::CType {
        // Create a CString from the string value, which can fail if there are embedded NUL bytes
        // in the string.  In that case we have a default string to fall back on
        let string = self.0;
        let cstr = self.1.get_or_insert_with(|| {
            match CString::new(string) {
                Ok(cstr) => {
                    //Unless `self.0` contains a NUL byte somewhere, this will succeed.
                    cstr
                }
                Err(_) => {
                    //This error means there was an embedded NUL byte in the string which can't be properly
                    //represented in a C string.  Use our default instead
                    CString::new("<probers-core failed to create CString>")
                        .expect("Failed to create static CString")
                }
            }
        });

        //Give the naked pointer to the CString.  This will be valid as long as this instance of the struct
        //is in scope
        cstr.as_ptr()
    }

    fn default_c_value() -> Self::CType {
        //In this case the default value is a null pointer.  Like it or not, it's a well-established convention
        //to use this value to indicate the absence of a real value
        0 as *const c_char
    }
}
