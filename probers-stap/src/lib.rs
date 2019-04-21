#![deny(warnings)]

//Only include any of this if stap is enabled for this build

#[cfg(enabled)]
pub mod probe;
#[cfg(enabled)]
pub mod provider;
#[cfg(enabled)]
pub mod tracer;

#[cfg(enabled)]
pub use probe::*;
#[cfg(enabled)]
pub use provider::*;
#[cfg(enabled)]
pub use tracer::*;
