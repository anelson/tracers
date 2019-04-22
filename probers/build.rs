//! Custom build logic that uses the features enabled by the dependent crate to determine which
//! tracing implementation to compile with
use failure::{bail, Fallible};
use std::env;

/// Struct which captures the features that were set in the `Cargo.toml` file of the dependent
/// crate
#[derive(Debug)]
struct Features {
    enable_tracing: bool,
    force_tracing: bool,
    force_dyn_stap: bool,
    force_noop: bool,
}

impl Features {
    pub fn from_env() -> Features {
        Features {
            enable_tracing: is_feature_enabled("enable_tracing"),
            force_tracing: is_feature_enabled("force_tracing"),
            force_dyn_stap: is_feature_enabled("force_dyn_stap"),
            force_noop: is_feature_enabled("force_noop"),
        }
    }
}

fn is_feature_enabled(name: &str) -> bool {
    env::var(&format!(
        "CARGO_FEATURE_{}",
        name.to_uppercase().replace("-", "_")
    ))
    .is_ok()
}

fn main() {
    for (name, value) in std::env::vars() {
        println!("{}={}", name, value);
    }

    let features = Features::from_env();

    println!("Detected features: \n{:?}", features);

    //by default we don't do anything here unless this lib is explicitly enabled
    if !features.enable_tracing {
        println!("probers is not enabled; build skipped");
        return;
    }

    match select_implementation(&features) {
        Ok(implementation) => {
            //Build succeeded, which means the Rust bindings should be enabled and
            //dependent crates should be signaled that this lib is available
            println!("cargo:rustc-cfg=enabled");
            println!("cargo:rustc-cfg={}_enabled", implementation);
            println!("cargo:succeeded=1"); //this will set DEP_(PKGNAME)_SUCCEEDED in dependent builds
        }
        Err(e) => {
            if features.force_tracing {
                panic!("probers build failed: {}", e);
            } else {
                println!("cargo:WARNING=probers-dyn-stap build failed: {}", e);
                println!(
                    "cargo:WARNING=the probers-dyn-stap bindings will not be included in the crate"
                );
            }
        }
    }
}

fn select_implementation(features: &Features) -> Fallible<&'static str> {
    //If any implementation is forced, then see if it's available and if so then accept it
    if features.force_dyn_stap {
        if env::var("DEP_PROBERS_DYN_STAP_SUCCEEDED").is_err() {
            bail!("force-dyn-stap is enabled but the dyn_stap library is not available")
        } else {
            return Ok("dyn_stap");
        }
    } else if features.force_noop {
        //no-op is always available on all platforms
        return Ok("native_noop");
    }

    //Else no tracing impl has been forced so we get to decide
    if env::var("DEP_PROBERS_DYN_STAP_SUCCEEDED").is_ok() {
        //use dyn_stap when it savailable
        return Ok("dyn_stap");
    } else {
        //else, fall back to noop
        return Ok("native_noop");
    }
}
