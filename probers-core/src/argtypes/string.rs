use super::reftype::{RefTypeConverter, RefTypeWrapper};
use super::ProbeArgType;
use std::ffi::CString;

/// Using the generic RefTypeWrapper implementation, we'll implement a wrapper that converts to a
/// CString.
pub struct StringConverter {}

impl RefTypeConverter<str, CString> for StringConverter {
    fn ref_to_primitive(arg: &str) -> Option<CString> {
        //Try to construct a CString from this Rust string.  If successful, return the new CString
        //wrapped in an Option.  If not return None which will be passed to the probe
        //infrastructure as a NULL pointer.
        CString::new(arg).ok()
    }
}

impl<'a> ProbeArgType<&'a str> for &'a str {
    type WrapperType = RefTypeWrapper<'a, str, CString, StringConverter>;
}
