//! This module implements the probing macros in such a way that at runtime they do nothing (hence
//! `noop` -- No Operation).  But they don't just compile down to nothing.
//!
//! It's important that even when the noop implementation is used, the same compile-time
//! verification applies as to the real implementatiosn.  Otherwise developers would do most of
//! their work with tracing disabled (meaning `noop`), then when they run into a problem that calls
//! for tracing, or do a release build with tracing enabled, they'd find their code is suddenly
//! broken.
//!
//! Thus this mode uses somme creative Rust trickery to generate code that ensures the compiler
//! does its usual type checks, but at runtime nothing actually happens.
