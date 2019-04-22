//! Custom build logic that auto generates the UnsafeProviderProbeNativeImpl that does nothing,
//! since this is the 'no-op' implementation
use failure::Fallible;
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;

const MAX_ARITY: usize = 12; //AFAIK Rust itself only allows tuples up to this arity

fn main() -> Fallible<()> {
    let out_dir = env::var("OUT_DIR").unwrap();
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
    let mut decl = r#"
        /// Implementation of `UnsafeProviderProbeNativeImpl` for NoOp.
        impl UnsafeProviderProbeNativeImpl for NoOpProbe
        {
            fn is_enabled(&self) -> bool { false }

            unsafe fn c_fire0(&self) { }

    "#
    .to_string();

    for arity in 1..=MAX_ARITY {
        let type_params = get_type_param_names(arity);
        let stap_type_params = type_params.clone();

        let stap_args = xform_types_i(&stap_type_params, |i, x| format!("_arg{}: {}", i, x));

        decl += &format!(
            r##"
            #[allow(clippy::duplicate_underscore_argument)]
            unsafe fn c_fire{arg_count}<{type_list}>(&self, {args})
                where {where_clause} {{
                }}
            "##,
            arg_count = type_params.len(),
            type_list = type_params.join(","),
            args = stap_args.join(","),
            where_clause = xform_types(&type_params, |x| format!(
                "{t}: ProbeArgNativeType<{t}>",
                t = x
            ))
            .join(","),
        );
    }

    decl += "}\n";

    decl
}
