#![deny(warnings)]
#![allow(dead_code)] //TODO: temporary
#![recursion_limit = "256"]

use failure::{format_err, Fallible};
use proc_macro2::Span;
use std::env;

mod argtypes;
mod cache;
mod cargo;
mod deps;
mod gen;
mod hashing;
mod probe_spec;
pub mod proc_macros;
mod provider_discovery;
mod syn_helpers;

#[cfg(test)]
mod testdata;

#[derive(Debug)]
pub struct ProberError {
    pub message: String,
    pub span: Span,
}

impl PartialEq<ProberError> for ProberError {
    fn eq(&self, other: &ProberError) -> bool {
        self.message == other.message
    }
}

impl ProberError {
    fn new<M: ToString>(message: M, span: Span) -> ProberError {
        ProberError {
            message: message.to_string(),
            span: span,
        }
    }
}

pub type ProberResult<T> = std::result::Result<T, ProberError>;

pub fn generate() -> Fallible<()> {
    let manifest_path = env::var("CARGO_MANIFEST_DIR").map_err(|_| {
        format_err!(
            "CARGO_MANIFEST_DIR is not set; are you sure you're calling this from within build.rs?"
        )
    })?;
    let package_name = env::var("CARGO_PKG_NAME").unwrap();
    let _targets = cargo::get_targets(&manifest_path, &package_name)?;

    unimplemented!();
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
