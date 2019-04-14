//! This module contains some shared code and structures for dealing with the test data in the
//! `testdata/` directory.  Each subfolder under `testdata` is a Rust crate complete with
//! `Cargo.toml`.  Each such crate contains a different combination of targets, source files, and
//! source file contents.  Some have errors such that they won't parse, most do not.
//!
//! The code here allows the various other modules in this crate to query the test data and know
//! what behavior to expect for each one.
#![cfg(test)]

use lazy_static::lazy_static;
use std::path::PathBuf;

pub(crate) struct Target {
    pub name: &'static str,

    /// The source file which a target will compile.  This is something like `src/lib.rs` or
    /// something in `tests/` or `examples/`.
    pub entrypoint: &'static str,

    /// Any additional files that are compiled by this target, meaning sub-modules or includes
    pub additional_source_files: Vec<&'static str>,
}

pub(crate) struct TestCase {
    pub root_directory: PathBuf,
    pub package_name: &'static str,
    pub targets: Vec<Target>,
}

lazy_static! {
    pub(crate) static ref TEST_CASE_DIR: PathBuf =
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata"));
    pub(crate) static ref TEST_CASES: Vec<TestCase> = vec![
        TestCase {
            root_directory: TEST_CASE_DIR.join("simplelib"),
            package_name: "simplelib",
            targets: vec![Target {
                name: "simplelib",
                entrypoint: "src/lib.rs",
                additional_source_files: vec!["src/child_module.rs"]
            }],
        },
        TestCase {
            root_directory: TEST_CASE_DIR.join("simplebin"),
            package_name: "simplebin",
            targets: vec![Target {
                name: "simplebin",
                entrypoint: "src/main.rs",
                additional_source_files: vec!["src/child_module.rs"]
            }],
        },
        TestCase {
            root_directory: TEST_CASE_DIR.join("complexlib"),
            package_name: "complexlib",
            targets: vec![
                Target {
                    name: "complexlib",
                    entrypoint: "src/lib.rs",
                    additional_source_files: vec![]
                },
                Target {
                    name: "bin1",
                    entrypoint: "src/bin/bin1.rs",
                    additional_source_files: vec![]
                },
                Target {
                    name: "bin2",
                    entrypoint: "src/bin/bin2.rs",
                    additional_source_files: vec![]
                },
                Target {
                    name: "ex1",
                    entrypoint: "examples/ex1.rs",
                    additional_source_files: vec![]
                },
                Target {
                    name: "test1",
                    entrypoint: "tests/test1.rs",
                    additional_source_files: vec![]
                },
                Target {
                    name: "test2",
                    entrypoint: "tests/test2.rs",
                    additional_source_files: vec![]
                },
            ],
        },
    ];
}
