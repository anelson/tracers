//! This module and its sub-modules contain structures which represent the parsed and processed
//! contents of the various probing macros, like `#[prober]` and `probe!`.
//!
//! Though there are multiple different probing implementations supported, the way the programmer
//! specifies providers and the way probes are fired must be the same regardless of implementation.
//! Thus, the implementatino doesn't control how to the `TokenStream` is intepreted; the proc
//! macros first call into these modules to parse the input `TokenStream` into a specification of
//! what the programmer wants, complete with helpful error messages if the specification is invalid
//! in some way.
//!
//! Only once a validated specification is available can the code generators go to work.
//!
//! Thus this can be seen as one side of the code generator implementation.  The other side, which
//! actually generates probing code, is in the `gen` module.
mod probe;
mod probe_arg;
mod probe_call;
mod provider;
mod provider_init;

pub use probe::ProbeSpecification;
pub use probe_arg::ProbeArgSpecification;
pub use probe_call::{ProbeCallDetails, ProbeCallSpecification};
pub use provider::ProviderSpecification;
pub use provider_init::ProviderInitSpecification;
