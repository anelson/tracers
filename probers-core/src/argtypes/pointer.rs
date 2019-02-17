use super::{ProbeArgType, ProbeArgWrapper};

impl<T> ProbeArgType<*const T> for *const T {
    type WrapperType = *const T;
    fn wrap(arg: *const T) -> Self::WrapperType {
        arg
    }
}

impl<T> ProbeArgWrapper for *const T {
    type CType = usize;

    fn as_c_type(&self) -> Self::CType {
        *self as usize
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
        assert_eq!(0usize, wrapper.as_c_type());
    }

    #[test]
    fn non_null_pointer() {
        struct Foo {
            _bar: i128,
        };
        let foo = Foo { _bar: 5 };
        let ptr: *const Foo = &foo;
        let wrapper = wrap(ptr);
        assert_eq!(ptr as usize, wrapper.as_c_type());
    }
}
