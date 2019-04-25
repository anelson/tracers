//! This module contains the code to support the static style of probing, which is more like the
//! static C support for user-mode tracing used by DTrace, SystemTap USDT, and DTrace USDT.  This
//! uses custom code running in the caller's `build.rs` to generate and compiler small C++ stubs
//! for each provider, which are implementing using the target tracing platform's own macros.  This
//! has a number of advantages over the implementation in `dynamic`:
//!
//! * The probes are embedded in the resulting binary, and can be discovered by system tools like
//! `tplist`, `bpftrace`, etc.
//! * This is the way the platform-specific tracing systems are intended to work.  Libraries that
//! implement dynamic tracing do so in a way that is, at best, a hack, and is poorly supported by
//! the static tools.
//! * This is much faster.  The Rust code will still incurr one method call when a probe is
//! enabled, which would not be incurred by equivalent static code.  But otherwise it's identical
//! to the static code implementation.  In limited cases when cross-language LTO is possible, the
//! optimizer can even remove this method call and produce the same performance as static C code.
//!
//! The only reason the `dynamic` implementation exists is that I wrote it first, before I figured
//! out how to make `static` work reliable.

pub(crate) mod noop;
