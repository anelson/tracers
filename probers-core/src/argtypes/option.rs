use super::{ProbeArgType, ProbeArgWrapper};
use std::fmt::Debug;

#[derive(Debug)]
pub struct OptionWrapper<T: ProbeArgType<T> + Debug> {
    inner_wrapper: Option<<T as ProbeArgType<T>>::WrapperType>,
}

impl<T> ProbeArgType<Option<T>> for Option<T>
where
    T: ProbeArgType<T> + Debug,
{
    type WrapperType = OptionWrapper<T>;
}

impl<T> ProbeArgWrapper<Option<T>> for OptionWrapper<T>
where
    T: ProbeArgType<T> + Debug,
{
    //When wrapping an Option<T>, the C type is the same as it would be for a T.
    //If there is no value for the Option<T>, we will use the default_c_value() value instead.
    type CType = <<T as ProbeArgType<T>>::WrapperType as ProbeArgWrapper<T>>::CType;

    fn new(arg: Option<T>) -> Self {
        let wrapped_arg =
            arg.map(|val| <<T as ProbeArgType<T>>::WrapperType as ProbeArgWrapper<T>>::new(val));
        OptionWrapper {
            inner_wrapper: wrapped_arg,
        }
    }

    fn to_c_type(&mut self) -> Self::CType {
        match &mut self.inner_wrapper {
            Some(wrapper) => wrapper.to_c_type(),
            None => Self::default_c_value(),
        }
    }

    fn default_c_value() -> Self::CType {
        <<T as ProbeArgType<T>>::WrapperType as ProbeArgWrapper<T>>::default_c_value()
    }
}
