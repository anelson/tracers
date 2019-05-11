#![deny(warnings)]
use tracers_macros::{init_provider, probe, tracer};

#[tracer]
trait TestProbes {
    fn probe0();
    fn probe1(foo: &str);
    fn probe2(foo: &str, bar: usize);
    fn unused_probe();
}

#[test]
fn probe_firing() {
    probe!(TestProbes::probe0());
    probe!(TestProbes::probe1("foo bar baz"));
    probe!(TestProbes::probe2("foo bar baz", 5));
}

#[test]
fn expected_impl() {
    //This very simple test checks the TRACERS_EXPECTED_PROVIDER env var, and if set, asserts that
    //the tracing implementation compiled into this library matches the expected one.  In
    //practice this is only used by the CI builds to verify that the compile-time magic always
    //ends up with the expeced implementation on a variety of environments
    if let Ok(expected_impl) = std::env::var("TRACERS_EXPECTED_PROVIDER") {
        match init_provider!(TestProbes) {
            Err(e) => panic!("Provider initialization error: {}", e),
            Ok(details) => assert_eq!(expected_impl, details),
        }
    }
}
