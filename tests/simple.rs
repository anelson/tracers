extern crate probers_macros;

use failure::Fallible;
use once_cell::sync::OnceCell;
use probe_rs::*;
use probers_macros::prober;
use std::thread;

#[prober]
pub trait MyProvider {
    fn probe2(&self, arg0: &str, arg1: u32) -> ();
}

pub struct MyProviderImpl<'a, 'b> {
    probe2: ProviderProbe<'a, SystemProbe, (&'b str, u32)>,
}

unsafe impl<'a, 'b> Send for MyProviderImpl<'a, 'b> {}
unsafe impl<'a, 'b> Sync for MyProviderImpl<'a, 'b> {}

impl<'a, 'b> MyProviderImpl<'a, 'b> {
    fn get() -> &'static Self {
        MYPROVIDER.get_or_init(|| {
            let provider = PROVIDER.get_or_init(|| {
                SystemTracer::define_provider("simple", |mut b| {
                    b.add_probe::<(i32, u32)>("probe2")?;
                    Ok(b)
                })
                .expect("Failed to create provider")
            });

            let probe2 = provider
                .get_probe::<(&str, u32)>("probe2")
                .expect("Failed to get probe");

            MyProviderImpl { probe2: probe2 }
        })
    }
}

impl<'a, 'b> MyProvider for MyProviderImpl<'a, 'b> {
    //fn probe2_is_enabled(&self) -> bool {
    //    self.probe2.is_enabled()
    //}

    fn probe2(&self, arg0: &str, arg1: u32) -> () {
        self.probe2.fire((arg0, arg1));
    }
}

static MYPROVIDER: OnceCell<MyProviderImpl> = OnceCell::INIT;
static PROVIDER: OnceCell<SystemProvider> = OnceCell::INIT;

fn main() -> Fallible<()> {
    //let _thread = thread::spawn(|| {
    //    let provider = MyProviderImpl::get();
    //
    //    for i in 0..100u32 {
    //        provider.probe2(&format!("arg0: {}", i), i);
    //    }
    //});
    //
    ////TODO: see if we can make the probe outlive the provider and cause a crash
    //let provider = MyProviderImpl::get();
    //assert_eq!(false, provider.probe2_is_enabled());
    //
    Ok(())
}
