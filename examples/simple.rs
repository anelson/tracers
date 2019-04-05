extern crate probers;

use probers::probe;
use probers::prober;

#[prober]
trait TestProbes {
    fn probe0();
    fn probe1(foo: &str);
}

fn main() {
    println!("About to fire the probes...");
    probe!(TestProbes::probe0());
    probe!(TestProbes::probe1("foo bar baz"));
    println!("The probes were fired");
}
