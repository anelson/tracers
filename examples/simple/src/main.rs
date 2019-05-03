//! This is a simple binary which declares and fires some simple probes
//!
//! It's the "hello world" equivalent for tracing
#![deny(warnings)]

use tracers_macros::{probe, tracer};

#[tracer]
trait SimpleProbes {
    fn hello(who: &str);
}

fn main() {
    loop {
        probe!(SimpleProbes::hello("world"));
    }
}
