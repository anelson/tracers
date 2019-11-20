# tracers - Rust instrumentation library

[![Azure Build Status - Linux/macOS/Windows](https://dev.azure.com/anelson-open-source/tracers/_apis/build/status/anelson.tracers?branchName=master)](https://dev.azure.com/anelson-open-source/tracers/_build/latest?definitionId=4&branchName=master)
[![Cirrus CI - FreeBSD](https://img.shields.io/cirrus/github/anelson/tracers)](https://cirrus-ci.com/github/anelson/tracers)

# Summary

`tracers` is intended to be an easy to use and cross-platform Rust crate which makes it easy to add high-performance
low-overhead probes to Rust programs.  Underneath it will use each platform's native probing mechanism, like System Tap
on Linux, DTrace on BSD, and ETW on Windows.  Those platforms without a supported probing mechanism will fall back to
a no-op implementation.

A key goal of this crate is to be able to drop it in to any Rust project, create and fire probes wherever it makes
sense, and leave those probes in place all the time.  When probes are disabled at compile time, there should be zero
runtime impact, and when probes are compiled in but not enabled at runtime the probe impact should be no more than one
or two CPU instructions.

# Status

*IMPORTANT*: `tracers` is still in the early experimental stages.  The author hereby guarantees it will not work on
your system, may cause data loss, and definitely contributes to premature hair loss.  Do not use it.

# License

Except where otherwise indicated, this project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

However, the following `-sys` crates have the license
corresponding to the third-party code which they wrap:

* [`tracers-libelf-sys`](tracers-libelf-sys/) - Wraps `elfutils` libraries and thus is licensed LGPLv3
* [`tracers-libstapsdt-sys`](tracers-libstapsdt-sys/) - Wraps `libstapsdt` and thus is licensed MIT

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in `tracers` by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

# Releasing

This section applies to maintainers only.

To release a new version, dependent crates must be released first.  The `bin/publish.sh` script helps to automate the
process but it's still quite manual.

Release process:

1. Update the `version` property of all crates and of all crates' dependencies on other `tracers` crates to the new
   target version.

1. Ensure all dependencies have both a path dependency for local development, and a version dependency for publishing.
   These must be consistent with the new version being published.

1. Update the `documentation` link to reflect the current version.

1. 

Crates must be published in this order:

* `tracers-core`
* `tracers-libelf-sys`
* `tracers-libstapsdt-sys`
* `tracers-codegen`
* `tracers-macros-hack`
* `tracers-macros`
* `tracers-dyn-stap`
* `tracers-dyn-noop`
* `tracers-build`
* `tracers`
