# tracers - Rust instrumentation library

[![Crates.io](https://img.shields.io/crates/v/tracers?style=plastic)](https://crates.io/crates/tracers)
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

*IMPORTANT*: `tracers` is still experimental.  The author is using it internally but this crate is still not yet widely
used and may contain subtle and critical defects.  

# Quick start

In your `Cargo.toml` you need to add:

    [dependencies]
    ...
    tracers = "0.1.0"
    tracers-macros = "0.1.0"

    [build-dependencies]
    ...
    tracers-build = "0.1.0"

It's important not to forget to add `tracers-build` to your `build-dependencies`, because you'll need that available at
build time for the next step, which is to create a `src/build.rs` file if you don't have one already, and make sure it
contains this:

    use tracers_build::build;

    fn main() {
	build();
    }

If you have an existing `build.rs` you'll need to make sure you add a call to `tracers_build::build()` somewhere in the
`main` function, preferably early.

At this point you have all you need to define a tracer.  Here's a simple example:

    use tracers_macros::{probe, tracer};

    #[tracer]
    trait SimpleProbes {
	fn hello(who: &str);
	fn greeting(greeting: &str, name: &str);
	fn optional_greeting(greeting: &str, name: &Option<&str>);
    }

    fn main() {
	loop {
	    probe!(SimpleProbes::hello("world"));
	    probe!(SimpleProbes::greeting("hello", "world"));
	    let name = Some("world");
	    probe!(SimpleProbes::optional_greeting("hello", &name));
	    let name: Option<&str> = None;
	    probe!(SimpleProbes::optional_greeting("hello", &name));
	}
    }

You have have defined three probes, `hello`, `greeting`, and `optional_greeting`.  By default, tracing is disabled at
compile time, so when you run this code all of the probing infrastructure will be optimized away and you'll be left with
zero runtime overhead.

To actually enable probing you need to activate one of the corresponding features in the `tracers` crate.  For example,
in your `Cargo.toml`:

    [dependencies]
    ...
    tracers = { version = "0.1.0", features = [ "force_static_stap"]

will enable SystemTap tracing.  If you rebuild again and use a tool like `tplist` from
[BCC](https://github.com/iovisor/bcc) you should be able to see the probes in the binary.

Note also that the `#[tracers]` macro generates some useful documentation on your trait.  Try `cargo doc` and find your
trait in the docs for additional hints on how to use each probe.

The `examples/` directory has some simple examples.

# Platforms

The `tracers` crate and runtime components should compile and run on any supported Rust platform (although `no_std` is
not yet supported).  Adding `tracers` as a dependency shouldn't break your project on any platform; if it does that's
a bug and you're encouraged to open a GitHub issue.

That said, the `tracers` crate by default doesn't actually trace anything; it compiles away to nothing.  To actually
enable tracing you need a supported platform.  As of this writing that means:

* Linux with System Tap (the `force_static_stap` feature)
* Linux with LTT-ng (the `force_static_lttng`) feature

There is work being done to support:

* Windows (with the Event Tracing for Windows system API)
* FreeBSD and macOS (with DTrace)


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
