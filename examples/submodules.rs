//! it's also possible to declare a provider struct in one module and fire the probes from another.
//! the only requirement is that the visibility of the provider struct be such that the struct type
//! is accessible from the code that is firing the probe
use probers::probe;

mod submodule_with_struct {
    extern crate probers;

    use probers_macros::prober;

    /// Note how this trait must be some kind of `pub`
    #[prober]
    pub trait PublicTestProbes {
        fn probe0();
        fn probe1(foo: &str);
    }

    #[prober]
    pub(crate) trait CrateTestProbes {
        fn probe0();
        fn probe1(foo: &str);
    }

    #[prober]
    pub(super) trait SuperTestProbes {
        fn probe0();
        fn probe1(foo: &str);
    }
}

use submodule_with_struct::*;

fn main() {
    println!("About to fire the probes...");
    probe!(PublicTestProbes::probe0());
    probe!(PublicTestProbes::probe1("foo bar baz"));
    probe!(CrateTestProbes::probe0());
    probe!(CrateTestProbes::probe1("foo bar baz"));
    probe!(SuperTestProbes::probe0());
    probe!(SuperTestProbes::probe1("foo bar baz"));
    println!("The probes were fired");
}
