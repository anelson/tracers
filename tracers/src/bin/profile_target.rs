//! This very simple binary is used for profiling the tracing system itself to experiment with
//! various optimizations and the effect they have on code generation.
//!
//! For actual benchmarking the code in the `benches/` directory is used instead
use std::ptr;
use tracers_macros::probe;
use tracers_macros::tracer;

#[tracer]
trait SimpleTestProbes {
    fn probe_no_args();
    fn probe_str_arg(arg0: &str);
    fn probe_int_arg(arg0: usize);
    fn probe_ptr_arg(arg0: *const u8);
}

static INT_ARG: usize = 52;
static STRING_ARG: &str = "foo bar baz";

fn main() {
    loop {
        unsafe {
            //TODO: use read_volatile to avoid compiler optimizations
            probe!(SimpleTestProbes::probe_no_args());
            probe!(SimpleTestProbes::probe_str_arg(ptr::read_volatile(
                &STRING_ARG
            )));
            probe!(SimpleTestProbes::probe_int_arg(ptr::read_volatile(
                &INT_ARG
            )));
            let ptr_arg: *const u8 = *&INT_ARG as *const u8;
            probe!(SimpleTestProbes::probe_ptr_arg(ptr::read_volatile(
                &ptr_arg
            )));
        }
    }
}
