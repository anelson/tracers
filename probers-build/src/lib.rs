//! There is no actual code for `probers-build`.  It's an exactly duplicate of `probers-codegen`,
//! but Cargo will treat it as a separate crate so it can be used as a `build-dependency` while
//! `probers-codegen` is a regular dependency.  That allows us to work around the nasty cargo bug
//! [#4866](https://github.com/rust-lang/cargo/issues/4866).

include!("../../probers-codegen/lib.rs")

