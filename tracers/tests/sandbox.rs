use tracers_macros::probe;
use tracers_macros::tracer;

extern crate tracers_macros;

#[tracer]
pub trait TestProbes {
    fn probe0();
    fn probe1(foo: &str);
}
#[test]
pub fn probe_firing() {
    probe!(TestProbes::probe0());
    probe!(TestProbes::probe1("foo bar baz"));
}

//pub struct TestProbes {}
//
//impl TestProbes {
//    pub fn probe0() {}
//
//    pub fn probe1(foo: &str) {
//        if false {
//            __impl_mod::ProbeArgTypeCheck::wrap(foo);
//        }
//    }
//
//    pub fn probe2(foo: std::path::PathBuf) {
//        if false {
//            __impl_mod::ProbeArgTypeCheck::wrap(foo)
//        }
//    }
//}
//
//mod __impl_mod {
//    use tracers::ProbeArgType;
//
//    pub(super) struct ProbeArgTypeCheck<T: ProbeArgType<T>> {
//        _t: ::std::marker::PhantomData<T>,
//    }
//
//    impl<T: ProbeArgType<T>> ProbeArgTypeCheck<T> {
//        #[allow(dead_code)]
//        pub fn wrap(arg: T) -> <T as ProbeArgType<T>>::WrapperType {
//            ::tracers::wrap::<T>(arg)
//        }
//    }
//}
