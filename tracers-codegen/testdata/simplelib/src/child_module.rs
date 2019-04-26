use tracers_macros::{tracer, probe};

#[tracer]
pub(super) trait MyTraceProvider {
    fn something_happend();

    fn something_else(foo: &str, bar: usize);
}

pub fn something() {
    probe!(MyTraceProvider::something_happend());
}
