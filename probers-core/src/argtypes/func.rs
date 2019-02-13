use super::{ProbeArgType, ProbeArgWrapper};
use std::fmt::Debug;

impl<'a, F, T> ProbeArgType<&'a F> for &'a F
where
    F: Fn() -> T,
    T: ProbeArgType<T> + Debug,
{
    type WrapperType = FuncProbeArgTypeWrapper<'a, F, T>;
}

pub struct FuncProbeArgTypeWrapper<'a, F, T>(&'a F, Option<<T as ProbeArgType<T>>::WrapperType>)
where
    F: Fn() -> T,
    T: ProbeArgType<T> + Debug;

impl<'a, F, T> ProbeArgWrapper<&'a F> for FuncProbeArgTypeWrapper<'a, F, T>
where
    F: Fn() -> T,
    T: ProbeArgType<T> + Debug,
{
    type CType = <<T as ProbeArgType<T>>::WrapperType as ProbeArgWrapper<T>>::CType;

    fn new(arg: &'a F) -> Self {
        FuncProbeArgTypeWrapper(arg, None)
    }

    fn to_c_type(&mut self) -> Self::CType {
        //If the wrapper for the function's return value hasn't been created yet, create it
        let func = self.0;

        //It's not enough to just call the underlying wrapper's to_c_type().  In some cases
        //(specifically with the StringWrapper for Rust strings), memory is allocated which is
        //freed when the wrapper goes out of scope.  Thus we have to keep it around, hence the
        //Option type.
        let wrapped = self.1.get_or_insert_with(|| {
            let arg = func();
            super::wrap(arg)
        });
        wrapped.to_c_type()
    }

    fn default_c_value() -> Self::CType {
        <<T as ProbeArgType<T>>::WrapperType as ProbeArgWrapper<T>>::default_c_value()
    }
}

impl<'a, F, T> Debug for FuncProbeArgTypeWrapper<'a, F, T>
where
    F: Fn() -> T,
    T: ProbeArgType<T> + Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        //Just use the Debug impl on the value returned by the function
        let arg = self.0();
        arg.fmt(f)
    }
}

//impl<CustomT, PrimitiveT> ProbeArgType<CustomT> for CustomT
//where
//    CustomT: CustomProbeArgType<PrimitiveT>,
//    PrimitiveT: ProbeArgType<PrimitiveT> + Sized + Debug,
//{
//    type WrapperType = CustomProbeArgWrapper<
//        CustomT,
//        PrimitiveT,
//        <PrimitiveT as ProbeArgType<PrimitiveT>>::WrapperType,
//    >;
//}
//
//pub struct CustomProbeArgWrapper<CustomT, PrimitiveT, PrimitiveWrapperT>
//where
//    CustomT: Into<PrimitiveT> + Sized,
//    PrimitiveWrapperT: ProbeArgWrapper<PrimitiveT>,
//    PrimitiveT: ProbeArgType<PrimitiveT> + Debug,
//{
//    arg: CustomT,
//    wrapper: Option<PrimitiveWrapperT>,
//}
//
//impl<CustomT, PrimitiveT, PrimitiveWrapperT> std::fmt::Debug
//    for CustomProbeArgWrapper<CustomT, PrimitiveT, PrimitiveWrapperT>
//where
//    CustomT: Into<PrimitiveT>,
//    PrimitiveWrapperT: ProbeArgWrapper<PrimitiveT>,
//    PrimitiveT: ProbeArgType<PrimitiveT> + Debug,
//{
//}
//
//impl<CustomT, PrimitiveT, PrimitiveWrapperT> ProbeArgWrapper<CustomT>
//    for CustomProbeArgWrapper<CustomT, PrimitiveT, PrimitiveWrapperT>
//where
//    CustomT: Into<PrimitiveT>,
//    PrimitiveWrapperT: ProbeArgWrapper<PrimitiveT>,
//    PrimitiveT: ProbeArgType<PrimitiveT> + Debug,
//{
//    type CType = PrimitiveWrapperT::CType;
//
//    fn to_c_type(&mut self) -> Self::CType {
//        let wrapped = super::wrap::<PrimitiveT>(self.0.into());
//        wrapped.to_c_type()
//    }
//    fn default_c_value() -> Self::CType {
//        PrimitiveWrapperT::default_c_value()
//    }
//}
