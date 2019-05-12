//! The default provider name is derived from the trait's name.  But this name is used to make the
//! names of the native code elements like semaphores.  So if two traits have the same provider
//! name it will result in linker errors.  If this compiles and runs it means the customer provider
//! name is being applied to the native code elements also
#![deny(warnings)]
use tracers_macros::probe;

mod foo {
    use tracers_macros::tracer;
    #[tracer(provider_name = "test_probes_1")]
    pub trait TestProbes {
        fn probe0();
        fn probe1(foo: &str);
    }
}

mod bar {
    use tracers_macros::tracer;
    #[tracer(provider_name = "test_probes_2")]
    pub trait TestProbes {
        fn probe1(foo: &str);
        fn probe2(foo: &str, bar: usize);
    }
}

#[test]
fn probe_firing() {
    probe!(foo::TestProbes::probe0());
    probe!(foo::TestProbes::probe1("foo bar baz"));
    probe!(bar::TestProbes::probe1("foo bar baz"));
    probe!(bar::TestProbes::probe2("foo bar baz", 5));
}
