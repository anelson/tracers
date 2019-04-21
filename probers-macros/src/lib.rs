//Just re-export the macros from the probers-macros-hack crate
use proc_macro_hack::proc_macro_hack;

#[proc_macro_hack]
pub use probers_macros_hack::probe;

#[proc_macro_hack]
pub use probers_macros_hack::init_provider;

pub use probers_macros_hack::prober;
