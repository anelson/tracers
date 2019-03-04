//extern crate probers_macros;
//
//use failure::Fallible;
//use once_cell::sync::OnceCell;
//use probe_rs::*;
//use probers_macros::prober;
//use std::thread;
//
//#[prober]
//pub trait MyProvider {
//    fn probe2(&self, arg0: &str, arg1: u32) -> ();
//}
//
//pub struct MyProviderImpl<'a, 'b> {
//    probe2: ProviderProbe<'a, SystemProbe, (&'b str, u32)>,
//}
//
//unsafe impl<'a, 'b> Send for MyProviderImpl<'a, 'b> {}
//unsafe impl<'a, 'b> Sync for MyProviderImpl<'a, 'b> {}
//
//impl<'a, 'b> MyProviderImpl<'a, 'b> {
//    fn get() -> &'static Self {
//        MYPROVIDER.get_or_init(|| {
//            let provider = PROVIDER.get_or_init(|| {
//                SystemTracer::define_provider("simple", |mut b| {
//                    b.add_probe::<(i32, u32)>("probe2")?;
//                    Ok(b)
//                })
//                .expect("Failed to create provider")
//            });
//
//            let probe2 = provider
//                .get_probe::<(&str, u32)>("probe2")
//                .expect("Failed to get probe
//
//            MyProviderImpl { probe2: probe2 }
//        })
//    }
//}
//
//impl<'a, 'b> MyProvider for MyProviderImpl<'a, 'b> {
//    //fn probe2_is_enabled(&self) -> bool {
//    //    self.probe2.is_enabled()
//    //}
//
//    fn probe2(&self, arg0: &str, arg1: u32) -> () {
//        self.probe2.fire((arg0, arg1));
//    }
//}
//
//static MYPROVIDER: OnceCell<MyProviderImpl> = OnceCell::INIT;
//static PROVIDER: OnceCell<SystemProvider> = OnceCell::INIT;
//
//fn main() -> Fallible<()> {
//    //let _thread = thread::spawn(|| {
//    //    let provider = MyProviderImpl::get();
//    //
//    //    for i in 0..100u32 {
//    //        provider.probe2(&format!("arg0: {}", i), i);
//    //    }
//    //});
//    //
//    ////TODO: see if we can make the probe outlive the provider and cause a crash
//    //let provider = MyProviderImpl::get();
//    //assert_eq!(false, provider.probe2_is_enabled());
//    //
//    Ok(())
//}

use probers_macros::prober;
trait Foo {
    #[deprecated(
        note = "Probe methods should not be called directly.  Use the `probe!` macro, e.g. `probe! Foo::probe0(...)`"
    )]
    fn probe0();
    fn probe0_enabled();
    fn probe0_impl();
    #[deprecated(
        note = "Probe methods should not be called directly.  Use the `probe!` macro, e.g. `probe! Foo::probe1(...)`"
    )]
    fn probe1(arg0: &str);
    fn probe1_enabled(arg0: &str);
    fn probe1_impl(arg0: &str);
}
mod foo_impl {
    use failure::{bail, Fallible};
    use once_cell::sync::OnceCell;
    use probers::{ProviderProbe, SystemProbe, SystemProvider, SystemTracer};
    use probers_core::{ProbeArgs, ProviderBuilder, Tracer};
    struct FooProviderImpl<'a, 'probe1_arg0> {
        probe0: ProviderProbe<'a, SystemProbe, ()>,
        probe1: ProviderProbe<'a, SystemProbe, (&'probe1_arg0 str,)>,
    }
    unsafe impl<'a, 'probe1_arg0> Send for FooProviderImpl<'a, 'probe1_arg0> {}
    unsafe impl<'a, 'probe1_arg0> Sync for FooProviderImpl<'a, 'probe1_arg0> {}
    static FOO_PROVIDER_IMPL: OnceCell<Fallible<FooProviderImpl>> = OnceCell::INIT;
    static FOO_PROVIDER: OnceCell<Fallible<SystemProvider>> = OnceCell::INIT;
    impl FooProviderImpl {
        fn get() -> &'static Fallible<FooProviderImpl> {
            let provider = FOO_PROVIDER.get_or_init(|| {
                SystemTracer::define_provider("sandbox::foo_impl", |mut builder| {
                    builder.add_probe::<()>("probe0")?;
                    builder.add_probe::<(&str,)>("probe1")?;
                    Ok(builder)
                })
            });
            FOO_PROVIDER_IMPL.get_or_init(move || match provider.as_ref() {
                Err(e) => {
                    return Err(::failure::err_msg("foo"));
                }
                Ok(p) => Ok(FooProviderImpl {}),
            })
        }
    }
}
