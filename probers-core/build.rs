//! Custom build script to programmatically generate support for tuples of up to 12 probe arguments.
//!
//! See the code in the `probes` module for more details.
use failure::Fallible;
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;

const MAX_ARITY: usize = 12; //AFAIK Rust itself only allows tuples up to this arity

fn main() -> Fallible<()> {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("probe_args.rs");
    let mut f = File::create(&dest_path)?;

    for arity in 0..=MAX_ARITY {
        let type_params = get_type_param_names(arity);
        f.write_all(generate_probe_args_impl(&type_params).as_bytes())?;
    }

    f.write_all(generate_unsafe_provider_probe_impl_trait().as_bytes())?;

    Ok(())
}

fn get_type_param_names(args: usize) -> Vec<String> {
    // Vector of all the type parameter names T0...Tn
    (0..args).map(|x| format!("T{}", x)).collect()
}

fn make_tuple_type(type_params: &Vec<String>) -> String {
    if type_params.is_empty() {
        "()".to_string()
    } else {
        format!("({},)", type_params.join(","))
    }
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

fn generate_probe_args_impl(type_params: &Vec<String>) -> String {
    let probe_args;
    let wrapper_decls;

    if type_params.is_empty() {
        wrapper_decls = "".to_string();
        probe_args = "".to_string();
    } else {
        wrapper_decls = xform_types_i(&type_params, |i, _| {
            format!("let wrapper{0} = wrap(self.{0})", i)
        })
        .join(";\n");
        probe_args =
            xform_types_i(&type_params, |i, _| format!("wrapper{}.as_c_type()", i)).join(",");;
    };

    format!(
        r#"
        /// Supports the use of a {arg_count}-arity tuple type of probe arguments.
        ///
        /// Note that all of the elements must have a type `T` for which `ProbeArgType<T>` is defined.
        impl<{type_list}> ProbeArgs<{tuple_type}> for {tuple_type} where {args_where_clause} {{
            const ARG_COUNT: usize = {arg_count};
            fn arg_types() -> Vec<CType> {{
                vec![{ctypes}]
            }}
            fn fire_probe<ImplT: UnsafeProviderProbeImpl>(self, probe: &ImplT) -> () {{
                {wrapper_decls};
               unsafe {{ probe.fire{arg_count}({probe_args}) }}
            }}
        }}
    "#,
        type_list = type_params.join(","),
        tuple_type = make_tuple_type(&type_params),
        args_where_clause = xform_types(&type_params, |x| format!("{t}: ProbeArgType<{t}>", t=x)).join(","),
        arg_count = type_params.len(),
        ctypes = xform_types(&type_params, |x| format!("get_ctype::<{}>()", x)).join(","),
        wrapper_decls = wrapper_decls,
        probe_args = probe_args
    )
}

/// Apologies for the confusing name.  We have a trait, UnsafeProviderProbeImpl,
/// which is implemented by the provider to fire the probe with a given set of args,
/// such that the provider does not verify the arg count or type.  Thus, it is unsafe.
/// The other layers in the API use the type system to ensure by the time a call gets
/// to this point, it has the correct number and type of arguments.
///
/// The implementor of this API for a specific tracing library need only implement all 13
/// possible `fire` methods, one for each number of args from 0 to 12.
fn generate_unsafe_provider_probe_impl_trait() -> String {
    let mut decl= r#"
        /// Apologies for the confusing name.  We have a trait, UnsafeProviderProbeImpl,
        /// which is implemented by the provider to fire the probe with a given set of args,
        /// such that the provider does not verify the arg count or type.  Thus, it is unsafe.
        /// The other layers in the API use the type system to ensure by the time a call gets
        /// to this point, it has the correct number and type of arguments.
        ///
        /// The implementor of this API for a specific tracing library need only implement all 13
        /// possible `fire` methods, one for each number of args from 0 to 12.
        ///
        /// *IMPORTANT NOTE TO IMPLEMENTORS*: Each of the `fireN` methods take arguments which may be either
        /// integers or possibly pointers to strings or other memory.  The caller guarantees that these are valid
        /// addresses *only* for the duration of the call.  Immediatley after the `fireN` method returns this memory may
        /// be freed.  Thus it's imperative that the probing implementation process probes synchronously.  Otherwise
        /// invalid memory accesses are inevitable.
        pub trait UnsafeProviderProbeImpl
        {
            /// Tests if this probe is enabled or not.  This should be a very fast test, ideally just a memory
            /// access.  The Rust compiler should be able to inline this implementation for maxmimum performance.
            fn is_enabled(&self) -> bool;

            unsafe fn fire0(&self) -> ();
    "#.to_string();

    for arity in 1..=MAX_ARITY {
        //For every possible arity level, declare a `probeN` method that takes a tuple of N
        //native (meaning C native) types to pass to the probe.
        let type_params = get_type_param_names(arity);

        decl += &format!(
            r##"
            unsafe fn fire{arg_count}<{type_list}>(&self, {args}) -> ()
                where {where_clause};
            "##,
            arg_count = type_params.len(),
            type_list = type_params.join(","),
            args = xform_types_i(&type_params, |i, x| format!("arg{}: {}", i, x)).join(","),
            where_clause = xform_types(&type_params, |x| format!(
                "{t}: ProbeArgNativeType<{t}>",
                t = x
            ))
            .join(",")
        );
    }

    decl.push_str("}");

    decl
}
