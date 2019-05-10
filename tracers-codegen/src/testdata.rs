//! This module contains some shared code and structures for dealing with the test data in the
//! `testdata/` directory.  Each subfolder under `testdata` is a Rust crate complete with
//! `Cargo.toml`.  Each such crate contains a different combination of targets, source files, and
//! source file contents.  Some have errors such that they won't parse, most do not.
//!
//! The code here allows the various other modules in this crate to query the test data and know
//! what behavior to expect for each one.
#![cfg(test)]

use crate::spec::ProbeCallDetails;
use crate::spec::ProbeCallSpecification;
use crate::spec::TracerAttribute;
#[cfg(target_os = "windows")]
use dunce::canonicalize; //on Windows the dunce implementation avoids UNC paths which break things
use fs_extra::{copy_items, dir};
use lazy_static::lazy_static;
use proc_macro2::TokenStream;
use quote::quote;
use std::env;
use std::fmt;
#[cfg(not(target_os = "windows"))]
use std::fs::canonicalize; //on non-Windows just use the built-in function
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use tempfile::tempdir;
use tracers_core::argtypes::{CType, ProbeArgNativeTypeInfo, ProbeArgType, ProbeArgWrapper};

type EnvVarsVec = Vec<(String, String, Option<String>)>;

lazy_static! {
    /// The `EnvVarsSetter` must only be used with the mutex held, otherwise the test runner can
    /// parallelize test runs and they can overwrite eachother's env vars
    static ref ENV_VARS_MUTEX: Mutex<EnvVarsVec> = Mutex::new(Vec::new());
}

fn unset_vars(vars: &mut EnvVarsVec) {
    println!("Restoring environment variables");
    for (key, _, old_val) in vars.iter() {
        if let Some(old_val) = old_val {
            println!("Restoring '{}' to '{}'", key, old_val);
            env::set_var(key, old_val)
        } else {
            println!("Unsetting '{}'", key);
            env::remove_var(key);
        }
    }

    vars.clear();
}

pub(crate) struct EnvVarsSetterGuard<'a> {
    guard: MutexGuard<'a, EnvVarsVec>,
}

impl<'a> EnvVarsSetterGuard<'a> {
    fn unset(&mut self) {
        unset_vars(&mut self.guard);
    }
}

impl<'a> Drop for EnvVarsSetterGuard<'a> {
    fn drop(&mut self) {
        //Unset the env vars before the guard is released
        self.unset();
    }
}

/// Sets the specified environment variables for the current process, and keeps them set until the
/// returned guard object is dropped.  Once it's dropped, the previous state of the environment
/// variables is restored.
///
/// Note that internally this uses a mutex to ensure there is only one thread in the process at a
/// time operating with modified environment variables.  Otherwise multiple threads could
/// interfere with the environment variables of the process and cause unpredictable behavior
pub(crate) fn with_env_vars<'a, K: AsRef<str>, V: AsRef<str>>(
    vars: Vec<(K, V)>,
) -> EnvVarsSetterGuard<'a> {
    println!("Acquiring mutex");
    let mut guard = match ENV_VARS_MUTEX.lock() {
        Err(e) => {
            println!("Mutex is poisoned; cleaning up previous env vars");
            e.into_inner()
        }
        Ok(guard) => guard,
    };

    //If there are any entries left in this vector, it suggest a previous test paniced and left
    //some variables set
    unset_vars(&mut guard);

    //Now the mutex is held and any previously un-restored env var overrides have been unwound.
    //The env vars now should be the state they were before we started to mess with them.  In this
    //state, grab the current value of each of the variables we are going to override, so we can
    //restore them later
    let vars: EnvVarsVec = vars
        .into_iter()
        .map(|(k, v)| {
            (
                k.as_ref().to_owned(),
                v.as_ref().to_owned(),
                env::var(k.as_ref()).ok(),
            )
        })
        .collect();

    //Push these new variables into the vector
    for variable in vars.into_iter() {
        guard.push(variable);
    }

    //And set the variables
    for (key, value, _) in guard.iter() {
        println!("Setting '{}' to '{}'", key, value);
        env::set_var(key, value);
    }

    EnvVarsSetterGuard { guard: guard }
}

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
    pub attr_tokenstream: TokenStream,
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
        attr_tokenstream: TokenStream,
        tokenstream: TokenStream,
        expected_error: &'static str,
    ) -> TestProviderTrait {
        TestProviderTrait {
            description,
            provider_name,
            attr_tokenstream,
            tokenstream,
            expected_error: Some(expected_error),
            probes: None,
        }
    }

    fn new_valid(
        description: &'static str,
        provider_name: &'static str,
        attr_tokenstream: TokenStream,
        tokenstream: TokenStream,
        probes: Vec<TestProbe>,
    ) -> TestProviderTrait {
        TestProviderTrait {
            description,
            provider_name,
            attr_tokenstream,
            tokenstream,
            expected_error: None,
            probes: Some(probes),
        }
    }

    pub fn get_attr_and_item_trait(&self) -> (TracerAttribute, syn::ItemTrait) {
        (
            syn::parse2(self.attr_tokenstream.clone()).expect("Expected valid tracer args"),
            syn::parse2(self.tokenstream.clone()).expect("Expected a valid trait"),
        )
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

#[derive(Debug)]
pub(crate) struct TestProbeCall {
    pub call: TokenStream,
    //The expected probe call if this token stream is valid, or a substring expected to be found in
    //the error if it's not valid
    pub expected: Result<ProbeCallSpecification, &'static str>,
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
    let default_attr_tokenstream = quote! { #[tracer] };
    let traits = vec![
        TestProviderTrait::new_valid(
            "empty trait",
            "test_trait",
            default_attr_tokenstream.clone(),
            quote! {
                trait TestTrait {}
            },
            vec![],
        ),
        TestProviderTrait::new_valid(
            "simple trait",
            "test_trait",
            default_attr_tokenstream.clone(),
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
            default_attr_tokenstream.clone(),
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
            default_attr_tokenstream.clone(),
            quote! {
                trait TestTrait<T: Debug> {
                }
            },
            "type parameter",
        ),
        TestProviderTrait::new_invalid(
            "has const",
            "test_trait",
            default_attr_tokenstream.clone(),
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
            default_attr_tokenstream.clone(),
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
            default_attr_tokenstream.clone(),
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
            default_attr_tokenstream.clone(),
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
            default_attr_tokenstream.clone(),
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
            default_attr_tokenstream.clone(),
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
            default_attr_tokenstream.clone(),
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
            default_attr_tokenstream.clone(),
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
            default_attr_tokenstream.clone(),
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
            default_attr_tokenstream.clone(),
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
            default_attr_tokenstream.clone(),
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
            default_attr_tokenstream.clone(),
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
            default_attr_tokenstream.clone(),
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
            default_attr_tokenstream.clone(),
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
            default_attr_tokenstream.clone(),
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
            default_attr_tokenstream.clone(),
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

/// Helper macro to help declare test probe calls
/// TODO: Add support for FireWithCode variations
macro_rules! test_probe_call {
    ($call:expr, @result $provider:path, $probe:path, $($arg:expr),*) => {
        TestProbeCall {
            call: quote! { $call },
            expected: Ok(
                ProbeCallSpecification::FireOnly(
                    ProbeCallDetails {
                        call: {
                            match ::syn::parse2::<syn::Expr>(quote! { $provider::$probe($($arg),*) }).unwrap(){
                                syn::Expr::Call(call) => call,
                                _ => {
                                    assert!(false, "The impossible happened!");
                                    unimplemented!()
                                }
                            }
                        },
                        probe_fq_path: ::syn::parse2::<syn::Path>(quote! { $provider::$probe }).unwrap(),
                        provider: ::syn::parse2::<syn::Path>(quote! { $provider }).unwrap(),
                        probe: ::syn::parse2::<syn::PathSegment>(quote! { $probe }).unwrap(),
                        args: vec![
                            $(
                                syn::parse2::<syn::Expr>(quote! { $arg }).unwrap()
                                ),*
                        ]
                    }
                    )
                )
        }
    };

    ($call:expr, @result $provider:path, $probe:path) => {
        test_probe_call!($call, @result $provider, $probe, )
    };

    ($call:expr, @error $error_msg:expr) => {
        TestProbeCall {
            call: quote! { $call },
            expected: Err($error_msg)
        }
    };
}

pub(crate) fn get_test_probe_calls() -> Vec<TestProbeCall> {
    vec![
        //test cases for a valid probe call
        test_probe_call!(MyProvider::my_probe(), @result MyProvider, my_probe),
        test_probe_call!(MyProvider::my_probe(arg0), @result MyProvider, my_probe, arg0),
        test_probe_call!(MyProvider::my_probe(&arg0), @result MyProvider, my_probe, &arg0),
        test_probe_call!(MyProvider::my_probe(someobj.callsomething().callsomethingelse(foo).unwrap()), @result MyProvider, my_probe, someobj.callsomething().callsomethingelse(foo).unwrap()),
        test_probe_call!(MyProvider::my_probe(somefunc(arg1, arg2, arg3)), @result MyProvider, my_probe, somefunc(arg1, arg2, arg3)),
        test_probe_call!(MyProvider::my_probe(arg0, arg1, arg3), @result MyProvider, my_probe, arg0, arg1, arg3),
        test_probe_call!(my_module::my_othermodule::my_foomodule::MyProvider::my_probe(arg0), @result my_module::my_othermodule::my_foomodule::MyProvider, my_probe, arg0),
        //various kinds of errors
        test_probe_call!(not_even_a_function_call, @error "requires the name of a provider trait and its probe method"),
        test_probe_call!(missing_provider(), @error "is missing the name of the provider trait"),
        test_probe_call!(MyProvider::not_even_a_function_call, @error "requires the name of a provider trait and its probe method"),
        test_probe_call!({ MyProvider::my_probe() }, @error "requires the name of a provider trait and its probe method"),
    ]
}

lazy_static! {
    pub(crate) static ref TEST_CRATE_DIR: PathBuf = {
        //NB: this will be invoked sometimes as part of the `tracers-build` crate, such that
        //`file!()` will have some path components like "../tracers-codegen/...".  That messes up
        //the tests which assume a literal path.  Thus, need to canonicalize
        let src_file = file!(); //This will be a path relative to the
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        //the src_file includes the name of the crate, like 'mycrate/src/lib.rs'.  Or, if this is
        //in tracers-build, where the path to the source file is in another crate, src_file will be
        //something like 'tracers-build/../tracers-codegen/src/lib.rs'.
        //
        //manifest_dir will be the path to the crate, like '/home/foo/tracers/tracers-build'
        //
        //So the parent of manifest_dir gets us the path to the workspace, which we can combine
        //with the src file's relative path to get the absolute path, then canonicalize
        let workspace_dir = manifest_dir.parent().expect("Manifest dir has no parent that's not possible");
        let src_path = workspace_dir.join(src_file);
        let mut src_dir = canonicalize(&src_path).expect(&format!("Failed to canonicalize source path: {}", &src_path.display()));
        src_dir.pop();

        let testdata_dir = src_dir.join("..").join("testdata");
        let testdata_dir = canonicalize(&testdata_dir).expect(&format!("Failed to canonicalize test data path: {}", &testdata_dir.display()));

        //At this point, `testdata_dir` is the fully qualified path on the filesystem to the
        //`testdata` directory in `tracers-codegen`.  The problem is that our test data include
        //complete crates with their own `Cargo.toml`.  When we run `cargo metadata` on those, it
        //will fail because Cargo will assume these are part of the `tracers` workspace.
        //
        //The only workaround is to create a temp directory OUTSIDE of the tracers source tree,
        //copy the data there, and use that.  it's...not pretty
        let temp_dir = tempdir().expect("Failed to create temporary directory").into_path();

        copy_items(&vec![testdata_dir.as_path()],
            temp_dir.as_path(),
            &dir::CopyOptions::new()).expect(&format!("Failed to copy {} to {}", testdata_dir.display(), temp_dir.display()));

        temp_dir.join("testdata").to_owned()
    };
    pub(crate) static ref TEST_CRATES: Vec<TestCrate> = vec![
        TestCrate {
            root_directory: TEST_CRATE_DIR.join("simplelib"),
            package_name: "simplelib",
            targets: vec![Target::new(
                "simplelib",
                "src/lib.rs",
                vec!["src/child_module.rs"],
                None
            ),
            Target::new("simplelib", "build.rs", vec![], None)],
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
                Target::new("test1", "tests/test1.rs", vec![ "tests/static/mod.rs"], None),
                Target::new("test2", "tests/test2.rs", vec![], None),
                Target::new("complexlib", "build.rs", vec![], None),
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
