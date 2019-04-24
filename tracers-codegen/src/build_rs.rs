//! This module contains the code that is used within `tracers`s build.rs` file to select
//! the suitable tracing implementation at build time, and within a dependent crate's `build.rs`
//! file to perform the build-time code generation to support the selected tracing implementation

use crate::cargo;
use crate::error::{TracersError, TracersResult};
use crate::TracingImplementation;
use crate::{CodeGenerator, Generator};
use failure::ResultExt;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::File;
use std::io::Write;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

/// Captures the features enabled for the build.  There are various combinations of them which
/// influence the logic related to what implementation is preferred
#[derive(Debug, Clone)]
struct FeatureFlags {
    enable_dynamic_tracing: bool,
    enable_native_tracing: bool,
    force_dyn_stap: bool,
    force_dyn_noop: bool,
}

impl FeatureFlags {
    /// Read the feature flags from the environment variables set by Cargo at build time.
    ///
    /// Fails with an error if the combination of features is not valid
    pub fn from_env() -> TracersResult<FeatureFlags> {
        Self::new(
            Self::is_feature_enabled("enable-dynamic-tracing"),
            Self::is_feature_enabled("enable-native-tracing"),
            Self::is_feature_enabled("force-dyn-stap"),
            Self::is_feature_enabled("force-dyn-noop"),
        )
    }

    /// Creates a feature flag structure from explicit arguments.  Mostly used for testing
    pub fn new(
        enable_dynamic_tracing: bool,
        enable_native_tracing: bool,
        force_dyn_stap: bool,
        force_dyn_noop: bool,
    ) -> TracersResult<FeatureFlags> {
        if enable_dynamic_tracing && enable_native_tracing {
            return Err(TracersError::code_generation_error("The features `enable-dynamic-tracing` and `enable-native-tracing` are mutually exclusive; please choose one"));
        }

        if force_dyn_stap && force_dyn_noop {
            return Err(TracersError::code_generation_error("The features `force-dyn-stap` and `force_dyn_noop` are mutually exclusive; please choose one"));
        }

        Ok(FeatureFlags {
            enable_dynamic_tracing,
            enable_native_tracing,
            force_dyn_stap,
            force_dyn_noop,
        })
    }

    pub fn enable_tracing(&self) -> bool {
        self.enable_dynamic() || self.enable_native()
    }

    pub fn enable_dynamic(&self) -> bool {
        self.enable_dynamic_tracing || self.force_dyn_noop || self.force_dyn_stap
    }

    pub fn enable_native(&self) -> bool {
        self.enable_native_tracing
    }

    pub fn force_dyn_stap(&self) -> bool {
        //Should the dynamic stap be required on pain of build failure?
        self.force_dyn_stap
    }

    pub fn force_dyn_noop(&self) -> bool {
        //Should the dynamic stap be required on pain of build failure?
        self.force_dyn_noop
    }

    fn is_feature_enabled(name: &str) -> bool {
        env::var(&format!(
            "CARGO_FEATURE_{}",
            name.to_uppercase().replace("-", "_")
        ))
        .is_ok()
    }
}

/// Serializable struct which is populated in `build.rs` to indicate to the proc macros which
/// tracing implementation they should use.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct BuildInfo {
    pub implementation: TracingImplementation,
}

impl BuildInfo {
    pub fn new(implementation: TracingImplementation) -> BuildInfo {
        BuildInfo { implementation }
    }

    pub fn load() -> TracersResult<BuildInfo> {
        let path = Self::get_build_path()?;

        let file = File::open(&path)
            .map_err(|e| TracersError::build_info_read_error(path.clone(), e.into()))?;
        let reader = BufReader::new(file);

        serde_json::from_reader(reader)
            .map_err(|e| TracersError::build_info_read_error(path.clone(), e.into()))
    }

    pub fn save(&self) -> TracersResult<PathBuf> {
        let path = Self::get_build_path()?;

        //Make sure the directory exists
        path.parent()
            .map(|p| {
                std::fs::create_dir_all(p)
                    .map_err(|e| TracersError::build_info_write_error(path.clone(), e.into()))
            })
            .unwrap_or(Ok(()))?;

        let file = File::create(&path)
            .map_err(|e| TracersError::build_info_write_error(path.clone(), e.into()))?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, self)
            .map_err(|e| TracersError::build_info_write_error(path.clone(), e.into()))?;

        Ok(path)
    }

    fn get_build_path() -> TracersResult<PathBuf> {
        //HACK: This is...not the most elegant solution.  This code gets used in three contexts:
        //
        //1. When `tracers` itself is being built, its `build.rs` calls into `tracers-build` to
        //   decide which probing implementation to use based on the feature flags specified by the
        //   caller.  In that case, `OUT_DIR` is set by `cargo` to the out directory for the
        //   `tracers` crate.  In that situation the `BuildInfo` file should go somewhere in
        //   `$OUT_DIR`, in a subdirectory named with the `tracers` package name and version
        //
        //2. When some other crate is using `tracers`, its `build.rs` calls into `tracers-build` to
        //   perform the build-time code generation tasks, which first require knowing which
        //   implementation `tracers` is using, hence requires reading the `BuildInfo` file.  In
        //   this case, `$OUT_DIR` is set to that dependent crate's output directory and is not
        //   what we want.  We want to know the path to the `BuildInfo` file produced when
        //   `tracers` was built.  Fortunately this information is passed on to the dependent crate
        //   by `tracers`s `build.rs` in the form of a cargo variable which cargo propagates to
        //   dependent crates as `DEP_TRACERS_BUILD_INFO_PATH`
        //
        //3. When one of the proc macros is invoked during compilation of some crate which is using
        //   `tracers`.  In this case, the other crate's `build.rs` should have already been run,
        //   and it should have called into the `tracers_build::build()` (that call is what creates
        //   context #2 above).  `tracers_build::build()` will instruct Cargo to set an environment
        //   variable `TRACERS_BUILD_INFO_PATH` during compilation, which this method will then
        //   example in order to get the path of the `BuidlInfo` build.
        //
        //I'm not proud of this code.  But `tracers` pushes the Rust build system just about to the
        //breaking point as it is.  We're lucky it's possible at all, hacks notwithstanding
        if "tracers" == env::var("CARGO_PKG_NAME").ok().unwrap_or_default() {
            //This is context #1 in the comment above: we're being called from within the `tracers`
            //build.rs
            let rel_path = PathBuf::from(&format!(
                "{}-{}/buildinfo.json",
                env::var("CARGO_PKG_NAME").context("CARGO_PKG_NAME")?,
                env::var("CARGO_PKG_VERSION").context("CARGO_PKG_VERSION")?
            ));

            Ok(PathBuf::from(env::var("OUT_DIR").context("OUT_DIR")?).join(rel_path))
        } else if let Ok(build_info_path) = env::var("DEP_TRACERS_BUILD_INFO_PATH") {
            //This is context #2 in the comment above
            Ok(PathBuf::from(build_info_path))
        } else if let Ok(build_info_path) = env::var("TRACERS_BUILD_INFO_PATH") {
            //This is context #3 in the comment above
            Ok(PathBuf::from(build_info_path))
        } else {
            //Since the first context happens in the `tracers` code itself and we know it's
            //implemented correctly, it means that this is either context #2 or #3 and the caller
            //did something wrong.  Most likely they forgot to add the call to
            //`tracers_build::build()` to their `build.rs`.  Since this is an easy mistake to make
            //we want an ergonomic error message here
            Err(TracersError::missing_call_in_build_rs())
        }
    }
}

/// Called from the `build.rs` of all crates which have a direct dependency on `tracers` and
/// `tracers_macros`.  This determines the compile-time configuration of the `tracers` crate, and
/// performs any build-time code generation necessary to support the code generated by the
/// `tracers_macros` macros.
///
/// It should be the first line in the `main()` function, etc:
///
/// ```no_execute
/// // build.rs
/// use tracers_build::build;
///
/// fn main() {
///     build();
///
///     //....
/// }
/// ```
pub fn build() {
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();

    let mut out_handle = stdout.lock();
    let mut err_handle = stderr.lock();

    match build_internal(&mut out_handle, &mut err_handle) {
        Ok(_) => writeln!(out_handle, "probes build succeeded").unwrap(),
        Err(e) => {
            //An error that propagates all the way up to here is serious enough that it means we
            //cannot proceed.  Fail the build by exiting the process forcefully
            writeln!(err_handle, "Error building probes: {}", e).unwrap();

            std::process::exit(-1);
        }
    };
}

fn build_internal<OUT: Write, ERR: Write>(out: &mut OUT, err: &mut ERR) -> TracersResult<()> {
    //First things first; get the BuildInfo from the `tracers` build, and tell Cargo to make that
    //available to the proc macros at compile time via an environment variable
    let build_info_path = BuildInfo::get_build_path()?;
    writeln!(
        out,
        "cargo:rustc-env=TRACERS_BUILD_INFO_PATH={}",
        build_info_path.display()
    )
    .unwrap();

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").context(
        "CARGO_MANIFEST_DIR is not set; are you sure you're calling this from within build.rs?",
    )?;

    let manifest_path = PathBuf::from(manifest_dir).join("Cargo.toml");
    let package_name = env::var("CARGO_PKG_NAME").unwrap();
    let targets = cargo::get_targets(&manifest_path, &package_name).context("get_targets")?;

    Generator::generate_native_code(out, err, &Path::new(&manifest_path), &package_name, targets)
}

/// This function is the counterpart to `build`, which is intended to be invoked in the `tracers`
/// `build.rs` script.  It reads the feature flags enabled on `tracers`, and from those flags and
/// other information about the target sytem and the local build environment selects an
/// implementation to use, or panics if no suitable implementation is possible
pub fn tracers_build() {
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();

    let mut out_handle = stdout.lock();
    let mut err_handle = stderr.lock();

    let features = FeatureFlags::from_env().expect("Invalid feature flags");

    match tracers_build_internal(&mut out_handle, features) {
        Ok(_) => {}
        Err(e) => {
            //failure here doesn't just mean one of the tracing impls failed to compile; when that
            //happens we can always fall back to the no-op impl.  This means something happened
            //which prevents us from proceeding with the build
            writeln!(err_handle, "{}", e).unwrap();
            panic!("tracers build failed: {}", e);
        }
    }
}

fn tracers_build_internal<OUT: Write>(out: &mut OUT, features: FeatureFlags) -> TracersResult<()> {
    writeln!(out, "Detected features: \n{:?}", features).unwrap();

    select_implementation(&features).map(|implementation| {
            //Build succeeded, which means the Rust bindings should be enabled and
            //dependent crates should be signaled that this lib is available
            writeln!(out,
                "cargo:rustc-cfg={}_enabled",
                if implementation.is_native() {
                    "native"
                } else {
                    "dynamic"
                }
            ).unwrap(); //this category of tracing is enabled
            writeln!(out, "cargo:rustc-cfg={}_enabled", implementation.as_ref()).unwrap(); //this specific impl is enabled

            //All downstream creates from `tracers` will just call `tracers_build::build`, but this
            //is a special case because we've already decided above which implementation to use.
            //
            //This decision needs to be saved to the OUT_DIR somewhere, so that all of our tests,
            //examples, binaries, and benchmarks which use the proc macros will be able to generate
            //the correct runtime tracing code to match the implementation we've chosen here
            let build_info = BuildInfo::new(implementation);
            match build_info.save() {
                Ok(build_info_path) => {
                    //The above statements set compile-time features to the compiler knows which modules to
                    //include.  The below will set environment variables DEP_TRACERS_(VARNAME) in dependent
                    //builds
                    //
                    //The codegen stuff in `tracers_build::build` will use this to determine what code
                    //generator to use
                    writeln!(out, "cargo:build-info-path={}", build_info_path.display()).unwrap();
                }
                Err(e) => {
                    writeln!(out, "cargo:WARNING=Error saving build info file; some targets may fail to build.  Error details: {}", e).unwrap();
                }
            }
    })
}

/// Selects a `tracers` implementation given a set of feature flags specified by the user
fn select_implementation(features: &FeatureFlags) -> TracersResult<TracingImplementation> {
    if !features.enable_tracing() {
        return Ok(TracingImplementation::NativeNoOp);
    }

    //If any implementation is forced, then see if it's available and if so then accept it
    if features.enable_dynamic() {
        // Pick some dynamic tracing impl
        if features.force_dyn_stap() {
            if env::var("DEP_TRACERS_DYN_STAP_SUCCEEDED").is_err() {
                return Err(TracersError::code_generation_error(
                    "force-dyn-stap is enabled but the dyn_stap library is not available",
                ));
            } else {
                return Ok(TracingImplementation::DynamicStap);
            }
        } else if features.force_dyn_noop() {
            //no-op is always available on all platforms
            return Ok(TracingImplementation::DynamicNoOp);
        }

        //Else no tracing impl has been forced so we get to decide
        if env::var("DEP_TRACERS_DYN_STAP_SUCCEEDED").is_ok() {
            //use dyn_stap when it savailable
            Ok(TracingImplementation::DynamicStap)
        } else {
            //else, fall back to noop
            Ok(TracingImplementation::DynamicNoOp)
        }
    } else {
        // Pick some static tracing impl
        Ok(TracingImplementation::NativeNoOp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdata;

    /// Helper that sets environment vars, then clears them when it goes out of scope
    struct EnvVarsSetter {
        vars: Vec<(String, String)>,
    }

    impl EnvVarsSetter {
        fn new<K: AsRef<str>>(vars: Vec<K>) -> EnvVarsSetter {
            let vars: Vec<_> = vars.into_iter().map(|k| (k, "1")).collect();

            Self::new_with_values(vars)
        }

        fn new_with_values<K: AsRef<str>, V: AsRef<str>>(vars: Vec<(K, V)>) -> EnvVarsSetter {
            let vars: Vec<_> = vars
                .into_iter()
                .map(|(k, v)| (k.as_ref().to_owned(), v.as_ref().to_owned()))
                .collect();

            for (key, value) in vars.iter() {
                env::set_var(key, value);
            }

            EnvVarsSetter { vars }
        }

        fn unset(&mut self) {
            for (key, _) in self.vars.iter() {
                env::remove_var(key);
            }

            self.vars.clear();
        }
    }

    impl Drop for EnvVarsSetter {
        fn drop(&mut self) {
            self.unset()
        }
    }

    #[test]
    #[should_panic]
    fn tracers_build_panics_invalid_features() {
        //These two feature flags are mutually exclusive
        let mut _setter = EnvVarsSetter::new(vec![
            "CARGO_FEATURE_ENABLE_NATIVE_TRACING",
            "CARGO_FEATURE_ENABLE_DYNAMIC_TRACING",
        ]);

        tracers_build();
    }

    #[test]
    fn build_rs_workflow_tests() {
        // Simulates the entire process, starting with `tracers_build` choosing an implementation
        // based on the selected feature flags, then the dependent crate calling `build` to query
        // the build info generated by `tracers_build` and to perform  perform
        // pre-processing of its code, then the proc macros reading the build info persisted by
        // `build` to generate the right implementation.
        //
        // This doesn't actually integrate all those systems in a test, but it simulates the
        // relevant calls into the `build_rs` code
        let test_cases = vec![
            //features, expected_impl
            (
                FeatureFlags::new(false, false, false, false).unwrap(),
                TracingImplementation::NativeNoOp,
            ),
            (
                FeatureFlags::new(true, false, false, false).unwrap(),
                TracingImplementation::DynamicNoOp,
            ),
            (
                FeatureFlags::new(false, true, false, false).unwrap(),
                TracingImplementation::NativeNoOp,
            ),
        ];

        let temp_dir = tempfile::tempdir().unwrap();

        let out_dir = temp_dir.path().join("out");

        for (features, expected_impl) in test_cases.into_iter() {
            //First let's present we're in `tracers/build.rs`, and cargo has set the relevant env
            //vars
            let vars = EnvVarsSetter::new_with_values(vec![
                ("CARGO_PKG_NAME", "tracers"),
                ("CARGO_PKG_VERSION", "1.2.3"),
                ("OUT_DIR", out_dir.to_str().unwrap()),
            ]);

            let mut stdout = Vec::new();

            tracers_build_internal(&mut stdout, features.clone())
                .expect(&format!("Unexpected failure with features: {:?}", features));

            //That worked.  The resulting build info should have been written out
            let build_info_path = BuildInfo::get_build_path().unwrap();

            let step1_build_info = BuildInfo::load().expect(&format!(
                "Failed to load build info for features: {:?}",
                features
            ));

            assert_eq!(expected_impl, step1_build_info.implementation);

            //And the path to this should have been written to stdout such that cargo will treat it
            //as a variable that is passed to dependent crates' `build.rs`:
            let output = String::from_utf8(stdout).unwrap();
            assert!(output.contains(&format!(
                "cargo:build-info-path={}",
                build_info_path.display()
            )));

            //Next, the user crate's `build.rs` will want to know what the selected impl was
            drop(vars);
            let vars = EnvVarsSetter::new_with_values(vec![(
                "DEP_TRACERS_BUILD_INFO_PATH",
                build_info_path.to_str().unwrap(),
            )]);

            let step2_build_info = BuildInfo::load().expect(&format!(
                "Failed to load build info for features: {:?}",
                features
            ));

            assert_eq!(step1_build_info, step2_build_info);

            //At this point in the process if this were a real build, the `build.rs` code would be
            //generating code for a real crate.  We're not going to simulate all of that here,
            //however we can invoke the code gen for all of our test crates at this point, and the
            //code gen should work using the currently selected implementatoin
            for test_case in testdata::TEST_CRATES.iter() {
                let context = format!(
                    "features: {:?} test_case: {}",
                    features,
                    test_case.root_directory.display()
                );

                let mut stdout = Vec::new();
                let mut stderr = Vec::new();

                let vars = EnvVarsSetter::new_with_values(vec![
                    ("CARGO_PKG_NAME", test_case.package_name),
                    (
                        "CARGO_MANIFEST_DIR",
                        test_case.root_directory.to_str().unwrap(),
                    ),
                ]);

                build_internal(&mut stdout, &mut stderr).expect(&context);

                //After the build, it should output something on stdout to tell Cargo to set a
                //compiler-visible env var telling the proc macros where the `BuildInfo` file is
                let output = String::from_utf8(stdout).unwrap();
                assert!(output.contains(&format!(
                    "cargo:rustc-env=TRACERS_BUILD_INFO_PATH={}",
                    build_info_path.display()
                )));

                drop(vars);
            }

            //That worked, next the proc macros will be run by `rustc` while it builds the user
            //crate.
            drop(vars);

            let vars = EnvVarsSetter::new_with_values(vec![(
                "TRACERS_BUILD_INFO_PATH",
                build_info_path.to_str().unwrap(),
            )]);

            let step3_build_info = BuildInfo::load().expect(&format!(
                "Failed to load build info for features: {:?}",
                features
            ));

            assert_eq!(step1_build_info, step3_build_info);

            drop(vars);
        }
    }
}
