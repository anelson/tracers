use super::reftype::{RefTypeConverter, RefTypeWrapper};
use super::ProbeArgType;
use std::ffi::CString;

/// Using the generic RefTypeWrapper implementation, we'll implement a wrapper that converts to a
/// CString.
pub struct StringConverter {}

impl RefTypeConverter<str, CString> for StringConverter {
    fn ref_to_primitive(arg: &str) -> CString {
        match CString::new(arg) {
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
    }
}

impl<'a> ProbeArgType<&'a str> for &'a str {
    type WrapperType = RefTypeWrapper<'a, str, CString, StringConverter>;
}
