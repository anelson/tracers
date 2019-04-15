#![deny(warnings)]

use failure::{format_err, Fallible};
use std::env;

mod cache;
mod cargo;
mod visitor;
mod hashing;

#[cfg(test)]
mod testdata;

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
