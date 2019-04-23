//! This module contains the code that is used within `probers`s build.rs` file to select
//! the suitable tracing implementation at build time, and within a dependent crate's `build.rs`
//! file to perform the build-time code generation to support the selected tracing implementation

use crate::cargo;
use crate::error::{ProbersError, ProbersResult};
use crate::TracingImplementation;
use crate::{CodeGenerator, Generator};
use failure::ResultExt;
use failure::{bail, Fallible};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::File;
use std::io::{stderr, stdout};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

/// Captures the features enabled for the build.  There are various combinations of them which
/// influence the logic related to what implementation is preferred
#[derive(Debug)]
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
    pub fn from_env() -> Fallible<FeatureFlags> {
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
    ) -> Fallible<FeatureFlags> {
        if enable_dynamic_tracing && enable_native_tracing {
            bail!("The features `enable-dynamic-tracing` and `enable-native-tracing` are mutually exclusive; please choose one")
        }

        if force_dyn_stap && force_dyn_noop {
            bail!("The features `force-dyn-stap` and `force_dyn_noop` are mutually exclusive; please choose one")
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
#[derive(Debug, Serialize, Deserialize)]
pub struct BuildInfo {
    pub implementation: TracingImplementation,
}

impl BuildInfo {
    pub fn new(implementation: TracingImplementation) -> BuildInfo {
        BuildInfo { implementation }
    }

    pub fn load() -> ProbersResult<BuildInfo> {
        let path = Self::get_build_path()?;

        let file = File::open(&path)
            .map_err(|e| ProbersError::build_info_read_error(path.clone(), e.into()))?;
        let reader = BufReader::new(file);

        serde_json::from_reader(reader)
            .map_err(|e| ProbersError::build_info_read_error(path.clone(), e.into()))
    }

    pub fn save(&self) -> ProbersResult<PathBuf> {
        let path = Self::get_build_path()?;

        //Make sure the directory exists
        path.parent()
            .map(|p| {
                std::fs::create_dir_all(p)
                    .map_err(|e| ProbersError::build_info_write_error(path.clone(), e.into()))
            })
            .unwrap_or(Ok(()))?;

        let file = File::create(&path)
            .map_err(|e| ProbersError::build_info_write_error(path.clone(), e.into()))?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, self)
            .map_err(|e| ProbersError::build_info_write_error(path.clone(), e.into()))?;

        Ok(path)
    }

    fn get_build_path() -> ProbersResult<PathBuf> {
        //HACK: This is...not the most elegant solution.  This code gets used in three contexts:
        //
        //1. When `probers` itself is being built, its `build.rs` calls into `probers-build` to
        //   decide which probing implementation to use based on the feature flags specified by the
        //   caller.  In that case, `OUT_DIR` is set by `cargo` to the out directory for the
        //   `probers` crate.  In that situation the `BuildInfo` file should go somewhere in
        //   `$OUT_DIR`, in a subdirectory named with the `probers` package name and version
        //
        //2. When some other crate is using `probers`, its `build.rs` calls into `probers-build` to
        //   perform the build-time code generation tasks, which first require knowing which
        //   implementation `probers` is using, hence requires reading the `BuildInfo` file.  In
        //   this case, `$OUT_DIR` is set to that dependent crate's output directory and is not
        //   what we want.  We want to know the path to the `BuildInfo` file produced when
        //   `probers` was built.  Fortunately this information is passed on to the dependent crate
        //   by `probers`s `build.rs` in the form of a cargo variable which cargo propagates to
        //   dependent crates as `DEP_PROBERS_BUILD_INFO_PATH`
        //
        //3. When one of the proc macros is invoked during compilation of some crate which is using
        //   `probers`.  In this case, the other crate's `build.rs` should have already been run,
        //   and it should have called into the `probers_build::build()` (that call is what creates
        //   context #2 above).  `probers_build::build()` will instruct Cargo to set an environment
        //   variable `PROBERS_BUILD_INFO_PATH` during compilation, which this method will then
        //   example in order to get the path of the `BuidlInfo` build.
        //
        //I'm not proud of this code.  But `probers` pushes the Rust build system just about to the
        //breaking point as it is.  We're lucky it's possible at all, hacks notwithstanding
        if "probers" == env::var("CARGO_PKG_NAME").ok().unwrap_or_default() {
            //This is context #1 in the comment above: we're being called from within the `probers`
            //build.rs
            let rel_path = PathBuf::from(&format!(
                "{}-{}/buildinfo.json",
                env::var("CARGO_PKG_NAME").context("CARGO_PKG_NAME")?,
                env::var("CARGO_PKG_VERSION").context("CARGO_PKG_VERSION")?
            ));

            Ok(PathBuf::from(env::var("OUT_DIR").context("OUT_DIR")?).join(rel_path))
        } else if let Some(build_info_path) = env::var("DEP_PROBERS_INFO_PATH").ok() {
            //This is context #2 in the comment above
            Ok(PathBuf::from(build_info_path))
        } else if let Some(build_info_path) = env::var("PROBERS_BUILD_INFO_PATH").ok() {
            //This is context #3 in the comment above
            Ok(PathBuf::from(build_info_path))
        } else {
            //Since the first context happens in the `probers` code itself and we know it's
            //implemented correctly, it means that this is either context #2 or #3 and the caller
            //did something wrong.  Most likely they forgot to add the call to
            //`probers_build::build()` to their `build.rs`.  Since this is an easy mistake to make
            //we want an ergonomic error message here
            Err(ProbersError::missing_call_in_build_rs())
        }
    }
}

/// Called from the `build.rs` of all crates which have a direct dependency on `probers` and
/// `probers_macros`.  This determines the compile-time configuration of the `probers` crate, and
/// performs any build-time code generation necessary to support the code generated by the
/// `probers_macros` macros.
///
/// It should be the first line in the `main()` function, etc:
///
/// ```
/// // build.rs
/// use probers_build::build;
///
/// fn main() {
///     build();
///
///     //....
/// }
/// ```
pub fn build() {
    match build_internal() {
        Ok(_) => println!("probes build succeeded"),
        Err(e) => eprintln!("Error building probes: {}", e),
    }
}

fn build_internal() -> ProbersResult<()> {
    let manifest_path = env::var("CARGO_MANIFEST_DIR").context(
        "CARGO_MANIFEST_DIR is not set; are you sure you're calling this from within build.rs?",
    )?;
    let package_name = env::var("CARGO_PKG_NAME").unwrap();
    let targets = cargo::get_targets(&manifest_path, &package_name).context("get_targets")?;

    let stdout = stdout();
    let stderr = stderr();

    let mut outhandle = stdout.lock();
    let mut errhandle = stderr.lock();

    Generator::generate_native_code(
        &mut outhandle,
        &mut errhandle,
        &Path::new(&manifest_path),
        &package_name,
        targets,
    )
}

/// This function is the counterpart to `build`, which is intended to be invoked in the `probers`
/// `build.rs` script.  It reads the feature flags enabled on `probers`, and from those flags and
/// other information about the target sytem and the local build environment selects an
/// implementation to use, or panics if no suitable implementation is possible
pub fn probers_build() {
    let features = FeatureFlags::from_env().unwrap();

    println!("Detected features: \n{:?}", features);

    //by default we don't do anything here unless this lib is explicitly enabled
    if !features.enable_tracing() {
        println!("probers is not enabled; build skipped");
        return;
    }

    match select_implementation(&features) {
        Ok(implementation) => {
            //Build succeeded, which means the Rust bindings should be enabled and
            //dependent crates should be signaled that this lib is available
            println!("cargo:rustc-cfg=enabled"); // tracing is enabled generally
            println!(
                "cargo:rustc-cfg={}_enabled",
                if implementation.is_native() {
                    "native"
                } else {
                    "dynamic"
                }
            ); //this category of tracing is enabled
            println!("cargo:rustc-cfg={}_enabled", implementation.as_ref()); //this specific impl is enabled

            //All downstream creates from `probers` will just call `probers_build::build`, but this
            //is a special case because we've already decided above which implementation to use.
            //
            //This decision needs to be saved to the OUT_DIR somewhere, so that all of our tests,
            //examples, binaries, and benchmarks which use the proc macros will be able to generate
            //the correct runtime tracing code to match the implementation we've chosen here
            let build_info = BuildInfo::new(implementation);
            match build_info.save() {
                Ok(build_info_path) => {
                    //The above statements set compile-time features to the compiler knows which modules to
                    //include.  The below will set environment variables DEP_PROBERS_(VARNAME) in dependent
                    //builds
                    //
                    //The codegen stuff in `probers_build::build` will use this to determine what code
                    //generator to use
                    println!("cargo:build-info-path={}", build_info_path.display());
                }
                Err(e) => {
                    println!("cargo:WARNING=Error saving build info file; some targets may fail to build.  Error details: {}", e);
                }
            }
        }
        Err(e) => {
            //failure here doesn't just mean one of the tracing impls failed to compile; when that
            //happens we can always fall back to the no-op impl.  This means something happened
            //which prevents us from proceeding with the build
            eprintln!("{}", e);
            panic!("probers build failed: {}", e);
        }
    }
}

/// Selects a `probers` implementation given a set of feature flags specified by the user
fn select_implementation(features: &FeatureFlags) -> Fallible<TracingImplementation> {
    //If any implementation is forced, then see if it's available and if so then accept it
    if features.enable_dynamic() {
        // Pick some dynamic tracing impl
        if features.force_dyn_stap() {
            if env::var("DEP_PROBERS_DYN_STAP_SUCCEEDED").is_err() {
                bail!("force-dyn-stap is enabled but the dyn_stap library is not available")
            } else {
                return Ok(TracingImplementation::DynamicStap);
            }
        } else if features.force_dyn_noop() {
            //no-op is always available on all platforms
            return Ok(TracingImplementation::DynamicNoOp);
        }

        //Else no tracing impl has been forced so we get to decide
        if env::var("DEP_PROBERS_DYN_STAP_SUCCEEDED").is_ok() {
            //use dyn_stap when it savailable
            return Ok(TracingImplementation::DynamicStap);
        } else {
            //else, fall back to noop
            return Ok(TracingImplementation::DynamicNoOp);
        }
    } else {
        // Pick some static tracing impl
        Ok(TracingImplementation::NativeNoOp)
    }
}
