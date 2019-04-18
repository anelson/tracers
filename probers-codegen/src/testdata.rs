//! This module contains some shared code and structures for dealing with the test data in the
//! `testdata/` directory.  Each subfolder under `testdata` is a Rust crate complete with
//! `Cargo.toml`.  Each such crate contains a different combination of targets, source files, and
//! source file contents.  Some have errors such that they won't parse, most do not.
//!
//! The code here allows the various other modules in this crate to query the test data and know
//! what behavior to expect for each one.
#![cfg(test)]

use lazy_static::lazy_static;
use probers_core::argtypes::{CType, ProbeArgNativeTypeInfo, ProbeArgType, ProbeArgWrapper};

use proc_macro2::TokenStream;
use quote::quote;
use std::fmt;
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

#[derive(Clone)]
pub(crate) struct TestProviderTrait {
    pub description: &'static str,
    pub provider_name: &'static str,
    pub tokenstream: TokenStream,
    pub expected_error: Option<&'static str>,
    pub probes: Option<Vec<TestProbe>>,
}

impl fmt::Debug for TestProviderTrait {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "TestProviderTrait(
    desc='{}',
    provider_name='{}',
    expected_error='{:?}',
    probes:\n",
            self.description, self.provider_name, self.expected_error
        )?;

        if let Some(ref probes) = self.probes {
            for probe in probes.iter() {
                write!(f, "        {:?},\n", probe)?;
            }
        }

        write!(f, ")")
    }
}

impl TestProviderTrait {
    fn new_invalid(
        description: &'static str,
        provider_name: &'static str,
        tokenstream: TokenStream,
        expected_error: &'static str,
    ) -> TestProviderTrait {
        TestProviderTrait {
            description,
            provider_name,
            tokenstream,
            expected_error: Some(expected_error),
            probes: None,
        }
    }

    fn new_valid(
        description: &'static str,
        provider_name: &'static str,
        tokenstream: TokenStream,
        probes: Vec<TestProbe>,
    ) -> TestProviderTrait {
        TestProviderTrait {
            description,
            provider_name,
            tokenstream,
            expected_error: None,
            probes: Some(probes),
        }
    }

    pub fn get_item_trait(&self) -> syn::ItemTrait {
        syn::parse2(self.tokenstream.clone()).expect("Expected a valid trait")
    }
}

#[derive(Clone)]
pub(crate) struct TestProbe {
    pub name: &'static str,
    pub args: Vec<(&'static str, syn::Type, CType)>,
}

impl fmt::Debug for TestProbe {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "TestProbe(name={}, args=(", self.name)?;

        for (name, typ, _) in self.args.iter() {
            write!(f, "{}: {},", name, quote! { #typ }.to_string())?;
        }

        write!(f, ")")
    }
}

impl TestProbe {
    fn new(name: &'static str, args: Vec<(&'static str, &'static str, CType)>) -> TestProbe {
        let args: Vec<_> = args
            .into_iter()
            .map(|(arg_name, rust_type, c_type)| {
                let rust_type: syn::Type =
                    syn::parse_str(rust_type).expect("Invalid Rust type expression");

                (arg_name, rust_type, c_type)
            })
            .collect();

        TestProbe { name, args }
    }
}

/// Helper macro allows us to express the details of a probe arg with a minimum of verbiage and
/// repetition
macro_rules! probe_arg {
    ($name:expr,$typ:ty) => {
        (
            $name,
            stringify!($typ),
            <<$typ as ProbeArgType<$typ>>::WrapperType as ProbeArgWrapper>::CType::get_c_type(),
        )
    };
}

//Alas, the types in `syn` and `proc_macro2` aren't Send+Sync, which means they can't be used as
//statics.  So the trait test data must be re-created by every test that needs it.  Pity.
pub(crate) fn get_test_provider_traits<F: FnMut(&TestProviderTrait) -> bool>(
    filter: impl Into<Option<F>>,
) -> Vec<TestProviderTrait> {
    let traits = vec![
        TestProviderTrait::new_valid(
            "empty trait",
            "test_trait",
            quote! {
                trait TestTrait {}
            },
            vec![],
        ),
        TestProviderTrait::new_valid(
            "simple trait",
            "test_trait",
            quote! {
                trait TestTrait {
                    fn probe0(arg0: i32);
                    fn probe1(arg0: &str);
                    fn probe2(arg0: &str, arg1: usize);
                }
            },
            vec![
                TestProbe::new("probe0", vec![probe_arg!("arg0", i32)]),
                TestProbe::new("probe1", vec![probe_arg!("arg0", &str)]),
                TestProbe::new(
                    "probe2",
                    vec![probe_arg!("arg0", &str), probe_arg!("arg1", usize)],
                ),
            ],
        ),
        TestProviderTrait::new_valid(
            "valid with many refs",
            "test_trait",
            quote! {
                    trait TestTrait {
                        fn probe0(arg0: i32);
                        fn probe1(arg0: &str);
                        fn probe2(arg0: &str, arg1: usize);
                        fn probe3(arg0: &str, arg1: &usize, arg2: &Option<i32>);
                    }

            },
            vec![
                TestProbe::new("probe0", vec![probe_arg!("arg0", i32)]),
                TestProbe::new("probe1", vec![probe_arg!("arg0", &str)]),
                TestProbe::new(
                    "probe2",
                    vec![probe_arg!("arg0", &str), probe_arg!("arg1", usize)],
                ),
                TestProbe::new(
                    "probe3",
                    vec![
                        probe_arg!("arg0", &str),
                        probe_arg!("arg1", &usize),
                        probe_arg!("arg2", &Option<i32>),
                    ],
                ),
            ],
        ),
        TestProviderTrait::new_invalid(
            "has trait type param",
            "test_trait",
            quote! {
                trait TestTrait<T: Debug> {
                }
            },
            "type parameter",
        ),
        TestProviderTrait::new_invalid(
            "has const",
            "test_trait",
            quote! {
                trait TestTrait {
                    fn probe0(arg0: i32);
                    const FOO: usize = 5;
                }
            },
            "no other contents",
        ),
        TestProviderTrait::new_invalid(
            "has type alias",
            "test_trait",
            quote! {
                trait TestTrait {
                    fn probe0(arg0: i32);
                    type Foo = Debug;
                }
            },
            "no other contents",
        ),
        TestProviderTrait::new_invalid(
            "has macro invocation",
            "test_trait",
            quote! {
                trait TestTrait {
                    println!("WTF");

                    fn probe0(arg0: i32);
                }
            },
            "no other contents",
        ),
        TestProviderTrait::new_invalid(
            "has const function",
            "test_trait",
            quote! {
                trait TestTrait {
                    const fn probe0(arg0: i32);
                }
            },
            "Probe methods cannot be",
        ),
        TestProviderTrait::new_invalid(
            "has unsafe function",
            "test_trait",
            quote! {
                trait TestTrait {
                    unsafe fn probe0(arg0: i32);
                }
            },
            "Probe methods cannot be",
        ),
        TestProviderTrait::new_invalid(
            "has extern function",
            "test_trait",
            quote! {
                trait TestTrait {
                    extern "C" fn probe0(arg0: i32);
                }
            },
            "Probe methods cannot be",
        ),
        TestProviderTrait::new_invalid(
            "has fn type param",
            "test_trait",
            quote! {
                trait TestTrait {
                    fn probe0<T: Debug>(arg0: T);
                }
            },
            "Probe methods must not take any type parameters",
        ),
        TestProviderTrait::new_invalid(
            "has explicit unit retval",
            "test_trait",
            quote! {
                trait TestTrait {
                    fn probe0(arg0: usize) -> ();
                }
            },
            "Probe methods must not have an explicit return",
        ),
        TestProviderTrait::new_invalid(
            "has non-unit retval",
            "test_trait",
            quote! {
                trait TestTrait {
                    fn probe0(arg0: usize) -> bool;
                }
            },
            "Probe methods must not have an explicit return",
        ),
        TestProviderTrait::new_invalid(
            "has default impl",
            "test_trait",
            quote! {
                trait TestTrait {
                    fn probe0(arg0: i32) { prinln!("{}", arg0); }
                }
            },
            "Probe methods must NOT have a default impl",
        ),
        TestProviderTrait::new_invalid(
            "has self method",
            "test_trait",
            quote! {
                trait TestTrait {
                    fn probe0(&self, arg0: i32);
                }
            },
            "Probe methods must not have any `&self`",
        ),
        TestProviderTrait::new_invalid(
            "has mut self method",
            "test_trait",
            quote! {
                trait TestTrait {
                    fn probe0(&mut self, arg0: i32);
                }
            },
            "Probe methods must not have any `&self`",
        ),
        TestProviderTrait::new_invalid(
            "has self by-val method",
            "test_trait",
            quote! {
                trait TestTrait {
                    fn probe0(self, arg0: i32);
                }
            },
            "Probe methods must not have any `self`",
        ),
        TestProviderTrait::new_invalid(
            "has mut self by-val method",
            "test_trait",
            quote! {
                trait TestTrait {
                    fn probe0(mut self, arg0: i32);
                }
            },
            "Probe methods must not have any `self`",
        ),
        TestProviderTrait::new_invalid(
            "has a nested Option parameter which is not supported",
            "test_trait",
            quote! {
                trait TestTrait {
                    fn probe0(arg0: &Option<Option<&str>>);
                }
            },
            "is not supported for probing",
        ),
        TestProviderTrait::new_invalid(
            "has a Result parameter which is not supported",
            "test_trait",
            quote! {
                trait TestTrait {
                    fn probe0(arg0: &Result<&str, &str>);
                }
            },
            "is not supported for probing",
        ),
    ];

    let filter = filter.into();
    if let Some(filter) = filter {
        traits.into_iter().filter(filter).collect()
    } else {
        traits
    }
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
