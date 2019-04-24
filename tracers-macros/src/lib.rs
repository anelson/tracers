//Just re-export the macros from the tracers-macros-hack crate
use proc_macro_hack::proc_macro_hack;

#[proc_macro_hack]
pub use tracers_macros_hack::probe;

#[proc_macro_hack]
pub use tracers_macros_hack::init_provider;

pub use tracers_macros_hack::tracer;
