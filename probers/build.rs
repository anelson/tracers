//! Custom build logic that uses the features enabled by the dependent crate to determine which
//! tracing implementation to compile with
use failure::{bail, Fallible};
use probers_build::build_rs::FeatureFlags;
use probers_build::TracingImplementation;
use std::env;

fn main() {
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
            println!("cargo:succeeded=1"); //this will set DEP_(PKGNAME)_SUCCEEDED in dependent builds

            //All downstream creates from `probers` will just call `probers_build::build`, but this
            //is a special case because we've already decided above which implementation to use.
            //
            //This decision needs to be saved to the OUT_DIR somewhere, so that all of our tests,
            //examples, binaries, and benchmarks which use the proc macros will be able to generate
            //the correct runtime tracing code to match the implementation we've chosen here
            let build_info = probers_build::build_rs::BuildInfo::new(implementation);
            if let Err(e) = build_info.save() {
                println!("cargo:WARNING=Error saving build info file; some targets may fail to build.  Error details: {}", e);
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
