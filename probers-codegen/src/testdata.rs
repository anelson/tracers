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

    /// Any expected errors in any of the files.  Each element of the vector is a tuple consisting
    /// of the source file path (relative to the root of the crate) and a string which should
    /// appear in an error message associated with that file
    pub expected_errors: Vec<(&'static str, &'static str)>,
}

impl Target {
    pub fn new(
        name: &'static str,
        entrypoint: &'static str,
        additional_source_files: Vec<&'static str>,
        expected_errors: Option<Vec<(&'static str, &'static str)>>,
    ) -> Target {
        Target {
            name,
            entrypoint,
            additional_source_files,
            expected_errors: expected_errors.unwrap_or(Vec::new()),
        }
    }
}

pub(crate) struct TestCrate {
    pub root_directory: PathBuf,
    pub package_name: &'static str,
    pub targets: Vec<Target>,
}

lazy_static! {
    pub(crate) static ref TEST_CRATE_DIR: PathBuf =
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata"));
    pub(crate) static ref TEST_CRATES: Vec<TestCrate> = vec![
        TestCrate {
            root_directory: TEST_CRATE_DIR.join("simplelib"),
            package_name: "simplelib",
            targets: vec![Target::new(
                "simplelib",
                "src/lib.rs",
                vec!["src/child_module.rs"],
                None
            )],
        },
        TestCrate {
            root_directory: TEST_CRATE_DIR.join("simplebin"),
            package_name: "simplebin",
            targets: vec![Target::new(
                "simplebin",
                "src/main.rs",
                vec!["src/child_module.rs"],
                None
            )],
        },
        TestCrate {
            root_directory: TEST_CRATE_DIR.join("complexlib"),
            package_name: "complexlib",
            targets: vec![
                Target::new("complexlib", "src/lib.rs", vec![], None),
                Target::new("bin1", "src/bin/bin1.rs", vec![], None),
                Target::new("bin2", "src/bin/bin2.rs", vec![], None),
                Target::new("ex1", "examples/ex1.rs", vec![], None),
                Target::new("test1", "tests/test1.rs", vec![], None),
                Target::new("test2", "tests/test2.rs", vec![], None),
            ],
        },
        TestCrate {
            root_directory: TEST_CRATE_DIR.join("errors"),
            package_name: "erroneous",
            targets: vec![
                Target::new(
                    "erroneous",
                    "src/main.rs",
                    vec!["src/child_mod/mod.rs", "src/child_mod/grandchild_mod.rs"],
                    Some(vec![(
                        "src/child_mod/grandchild_mod.rs",
                        "this_mod_doesnt_exist"
                    )])
                ),
                Target::new(
                    "compile_errors",
                    "tests/compile_errors.rs",
                    vec![],
                    Some(vec![("tests/with_errors/mod.rs", "expected `!")])
                ),
            ],
        },
    ];
}
