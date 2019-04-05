# probe-rs - Rust instrumentation library

[![Build Status](https://dev.azure.com/anelson-open-source/probe-rs/_apis/build/status/anelson.probe-rs?branchName=master)](https://dev.azure.com/anelson-open-source/probe-rs/_build/latest?definitionId=1&branchName=master)

# Summary

`probe-rs` is intended to be an easy to use and cross-platform Rust crate which makes it easy to add high-performance
low-overhead probes to Rust programs.  Underneath it will use each platform's native probing mechanism, like System Tap
on Linux, DTrace on BSD, and ETW on Windows.  Those platforms without a supported probing mechanism will fall back to
a no-op implementation.

A key goal of this crate is to be able to drop it in to any Rust project, create and fire probes wherever it makes
sense, and leave those probes in place all the time.  When probes are disabled at compile time, there should be zero
runtime impact, and when probes are compiled in but not enabled at runtime the probe impact should be no more than one
or two CPU instructions.

# Status

*IMPORTANT*: `probe-rs` is still in the early experimental stages.  The author hereby guarantees it will not work on
your system, may cause data loss, and definitely contributes to premature hair loss.  Do not use it.

# License

Except where otherwise indicated, this project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

However, the following `-sys` crates have the license
corresponding to the third-party code which they wrap:

* [`libelf-sys`](libelf-sys/) - Wraps `elfutils` libraries and thus is licensed LGPLv3
* [`libstapsdt-sys`](libstapsdt-sys/) - Wraps `libstapsdt` and thus is licensed MIT

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in `probe-rs` by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.


