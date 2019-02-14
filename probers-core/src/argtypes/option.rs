use super::reftype::{RefTypeConverter, RefTypeWrapper};
use super::{ProbeArgTraits, ProbeArgType};

/// Using the generic RefTypeWrapper implementation, we'll implement a wrapper that converts to a
/// T from Option<T> assuming T itself is supported as a ProbeArgType<T>
pub struct OptionConverter {}

impl<T> RefTypeConverter<Option<T>, T> for OptionConverter
where
    T: ProbeArgTraits<T> + Copy,
{
    fn ref_to_primitive(arg: &Option<T>) -> Option<T> {
        //This trait was designed for cases where the conversion to the primitive tyep was
        //fallible and thus could result in None.  In the case of the Option type that's
        //literallyhow it's implemented.
        *arg
    }
}

impl<'a, T> ProbeArgType<&'a Option<T>> for &'a Option<T>
where
    T: ProbeArgTraits<T> + Copy,
{
    type WrapperType = RefTypeWrapper<'a, Option<T>, T, OptionConverter>;
}
