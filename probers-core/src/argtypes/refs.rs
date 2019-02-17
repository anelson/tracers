//! This module ensures that if `ProbeArgTraits` and `ProbeArgWrapper` are implemented for some `T`, then they are also implemented for `&T`.
//! This is necessary for convenience and also to support our generialized implementation for `Option<T>`

use super::{ProbeArgTraits, ProbeArgType};
use std::marker::Copy;

impl<'a, T> ProbeArgType<&'a T> for &'a T
where
    T: ProbeArgTraits<T> + Copy,
{
    type WrapperType = <T as ProbeArgType<T>>::WrapperType;

    fn wrap(arg: &'a T) -> Self::WrapperType {
        super::wrap(*arg)
    }
}

#[cfg(test)]
mod test {
    use crate::wrap;

    #[test]
    fn ref_str() {
        let string: &str = "foo bar baz";
        let ref_to_string: &&str = &string;

        let wrapper = wrap(string);
        let ref_wrapper = wrap(ref_to_string);

        assert_eq!(wrapper, ref_wrapper);
    }

    #[test]
    fn ref_int() {
        let value = 5usize;
        let ref_to_value = &value;

        let wrapper = wrap(value);
        let ref_wrapper = wrap(ref_to_value);

        assert_eq!(wrapper, ref_wrapper);
    }
}
