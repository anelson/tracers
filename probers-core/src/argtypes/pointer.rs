use super::{ProbeArgType, ProbeArgWrapper};
use std::os::raw;

impl<T> ProbeArgType<*const T> for *const T {
    type WrapperType = *const T;
    fn wrap(arg: *const T) -> Self::WrapperType {
        arg
    }
}

impl<T> ProbeArgWrapper for *const T {
    type CType = *const raw::c_void;

    fn as_c_type(&self) -> Self::CType {
        *self as *const raw::c_void
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use std::ptr;

    #[test]
    fn null_pointer() {
        let ptr: *const u8 = ptr::null();
        let wrapper = wrap(ptr);
        assert_eq!(ptr::null(), wrapper.as_c_type());
    }

    #[test]
    fn non_null_pointer() {
        struct Foo {
            _bar: i128,
        };
        let foo = Foo { _bar: 5 };
        let ptr: *const Foo = &foo;
        let wrapper = wrap(ptr);
        assert_eq!(ptr as *const raw::c_void, wrapper.as_c_type());
    }
}
