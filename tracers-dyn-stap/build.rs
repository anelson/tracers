//! Custom build logic that auto generates the UnsafeProviderProbeNativeImpl for SystemTap, one method for
//! each possible arg count from 0 to 12.
use failure::{bail, Fallible};
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;

const MAX_ARITY: usize = 12; //AFAIK Rust itself only allows tuples up to this arity
const STAP_MAX_ARITY: usize = 6; //any more than this number of probe arguments are allowed but are ignored

fn is_enabled() -> bool {
    env::var("CARGO_FEATURE_ENABLED").is_ok() || is_required()
}

fn is_required() -> bool {
    env::var("CARGO_FEATURE_REQUIRED").is_ok()
}

fn main() {
    println!("building tracers-dyn-stap if enabled");

    //by default we don't do anything here unless this lib is explicitly enabled
    if !is_enabled() {
        println!("tracers-dyn-stap is not enabled; build skipped");
        return;
    }

    let fail_on_error = is_required();

    match try_build() {
        Ok(_) => {
            //Build succeeded, which means the Rust bindings should be enabled and
            //dependent crates should be signaled that this lib is available
            println!("cargo:rustc-cfg=enabled");
            println!("cargo:succeeded=1"); //this will set DEP_(PKGNAME)_SUCCEEDED in dependent builds
        }
        Err(e) => {
            if fail_on_error {
                panic!("tracers-dyn-stap build failed: {}", e);
            } else {
                println!("cargo:WARNING=tracers-dyn-stap build failed: {}", e);
                println!(
                    "cargo:WARNING=the tracers-dyn-stap bindings will not be included in the crate"
                );
            }
        }
    }
}

fn try_build() -> Fallible<()> {
    if env::var("DEP_STAPSDT_SUCCEEDED").is_err() {
        bail!("tracers-dyn-stap is not available because libstapsdt-sys did not build successfully")
    }

    let out_dir = env::var("OUT_DIR")?;
    let dest_path = Path::new(&out_dir).join("probe_unsafe_impl.rs");
    let mut f = File::create(&dest_path)?;

    f.write_all(generate_stap_native_impl().as_bytes())?;

    Ok(())
}

fn get_type_param_names(args: usize) -> Vec<String> {
    // Vector of all the type parameter names T0...Tn
    (0..args).map(|x| format!("T{}", x)).collect()
}

fn xform_types<F: FnMut(&String) -> String>(type_params: &Vec<String>, mut f: F) -> Vec<String> {
    type_params.iter().map(|x| f(x)).collect::<Vec<String>>()
}

fn xform_types_i<F: FnMut(usize, &String) -> String>(
    type_params: &Vec<String>,
    mut f: F,
) -> Vec<String> {
    type_params
        .iter()
        .enumerate()
        .map(|(i, x)| f(i, x))
        .collect::<Vec<String>>()
}

fn generate_stap_native_impl() -> String {
    let mut decl= r#"
        /// Implementation of `UnsafeProviderProbeNativeImpl` for SystemTap.
        ///
        /// NB: While the `tracers` API supports probes with from 0 to 12 arguments, the libstapsdt library (or maybe SystemTap itself)
        /// support up to 6.  This implementation must provide all arities from 0 to 12, but only the first 6 parameters are used.
        impl UnsafeProviderProbeNativeImpl for StapProbe
        {
            fn is_enabled(&self) -> bool { StapProbe::is_enabled(self) }

            unsafe fn c_fire0(&self) {
                probeFire(self.probe);
            }

    "#.to_string();

    for arity in 1..=MAX_ARITY {
        //For every possible arity level `N`, declare the probe method `c_fireN` which takes C native argument types
        //because SystemTap can only accept up to STAP_MAX_ARITY arguments, any ones after that need to have a leading
        //underscore to mark them as unused
        let stap_arg_count = if arity > STAP_MAX_ARITY {
            STAP_MAX_ARITY
        } else {
            arity
        };

        let type_params = get_type_param_names(arity);
        let mut stap_type_params = type_params.clone();
        let ignored_type_params = stap_type_params.split_off(stap_arg_count);

        let stap_args = xform_types_i(&stap_type_params, |i, x| format!("arg{}: {}", i, x));
        let ignored_type_args =
            xform_types_i(&ignored_type_params, |i, x| format!("_arg{}: {}", i, x));
        let mut all_args = stap_args.clone();
        all_args.extend(ignored_type_args.clone());
        let stap_arg_names = xform_types_i(&stap_type_params, |i, _| format!("arg{}", i));

        decl += &format!(
            r##"
            #[allow(clippy::duplicate_underscore_argument)]
            unsafe fn c_fire{arg_count}<{type_list}>(&self, {args})
                where {where_clause} {{
                  probeFire(self.probe, {stap_arg_names});
                }}
            "##,
            arg_count = type_params.len(),
            type_list = type_params.join(","),
            args = all_args.join(","),
            where_clause = xform_types(&type_params, |x| format!(
                "{t}: ProbeArgNativeType<{t}>",
                t = x
            ))
            .join(","),
            stap_arg_names = stap_arg_names.join(",")
        );
    }

    decl += "}\n";

    decl
}
