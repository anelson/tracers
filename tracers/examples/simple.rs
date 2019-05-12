#![deny(warnings)]

use tracers_macros::probe;
use tracers_macros::tracer;

#[tracer]
trait SimpleTestProbes {
    fn probe0();
    fn probe1(foo: &str);
}

fn main() {
    println!("About to fire the probes...");
    probe!(SimpleTestProbes::probe0());
    probe!(SimpleTestProbes::probe1("foo bar baz"));
    println!("The probes were fired");
}
