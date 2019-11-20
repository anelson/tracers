//! Custom build script to programmatically generate support for tuples of up to 12 probe arguments.
//!
//! This may be some the gnarliest code I've written.  It's as messy and unsatisfying as most code
//! generators I've had to write.  Thankfully most of the ugliness is in the code that generates
//! the tests, so there's at least some automated eyes looking over my shoulder.
//!
//! See the code in the `probes` module for more details.
use failure::Fallible;
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;

const MAX_ARITY: usize = 12; //AFAIK Rust itself only allows tuples up to this arity
const MAX_QUICKCHECK_ARITY: usize = 8; //this is an unfortunate limit.

fn main() -> Fallible<()> {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("probe_args.rs");
    let dest_tests_path = Path::new(&out_dir).join("probe_args_tests.rs");
    let mut f = File::create(&dest_path)?;
    let mut f_tests = File::create(&dest_tests_path)?;

    for arity in 0..=MAX_ARITY {
        let type_params = get_type_param_names(arity);
        f.write_all(generate_probe_args_impl(&type_params).as_bytes())?;
    }

    f.write_all(generate_unsafe_provider_probe_impl_trait().as_bytes())?;
    f.write_all(generate_unsafe_provider_probe_native_impl_trait().as_bytes())?;
    f_tests.write_all(generate_tests().as_bytes())?;

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
    let probe_args = xform_types_i(&type_params, |i, _| format!("self.{}", i));

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

            fn fire_probe<ImplT: UnsafeProviderProbeImpl>(self, probe: &ImplT) {{
               unsafe {{ probe.fire{arg_count}({probe_args}) }}
            }}
        }}
    "#,
        type_list = type_params.join(","),
        tuple_type = make_tuple_type(&type_params),
        args_where_clause = xform_types(&type_params, |x| format!("{t}: ProbeArgType<{t}>", t=x)).join(","),
        arg_count = type_params.len(),
        ctypes = xform_types(&type_params, |x| format!("get_ctype::<{}>()", x)).join(","),
        probe_args = probe_args.join(",")
    )
}

/// This generates the declaration of the `UnsafeProviderProbeImpl` trait.  This trait implements tracing
/// for implementations that will operate on Rust types, not native C types.  The intention of this library
/// is to support C native tracing libraries, so the only direct implementation of this trait is likely to be
/// an implementation which simply logs the probe arguments to Rust's `logging` crate, for debugging purposes
/// or use on platforms without a supported tracing library.
fn generate_unsafe_provider_probe_impl_trait() -> String {
    let mut decl= r#"
        /// This trait is implemented by tracing providers that operate on probe arguments in their Rust represention, not
        /// their C native representation.  Most implementations will not implement this trait directly but rather its subtrait
        /// `UnsafeProviderProbeNativeImpl`, which provides an implementation of this trait which wraps all parameters in their
        /// wrapper types and calls its own trait methods with native C representations of each argument.
        ///
        /// This trait and its subtrait `UnsafeProviderProbeNativeImpl` are both unsafe because the provider cannot necessarily
        /// verify that the types and argument counts for the probe match those when the probe was first created.  This is partially for
        /// performance reasons and also a practical limitation of the `var-arg` based implementations most commonly used in C tracing
        /// libraries.
        ///
        /// The implementor of this API for a specific tracing library need only implement all 13
        /// possible `fire` methods, one for each number of args from 0 to 12.
        #[allow(clippy::too_many_arguments)]
        pub trait UnsafeProviderProbeImpl
        {
            /// Tests if this probe is enabled or not.  This should be a very fast test, ideally just a memory
            /// access.  The Rust compiler should be able to inline this implementation for maxmimum performance.
            fn is_enabled(&self) -> bool;

            unsafe fn fire0(&self);
    "#.to_string();

    for arity in 1..=MAX_ARITY {
        //For every possible arity level, declare a `probeN` method that takes a tuple of N
        //`ProbeArgType<T>` types.  That is, types that are not wrapped yet but are passed as-is as Rust types.
        let type_params = get_type_param_names(arity);

        decl += &format!(
            r##"
            unsafe fn fire{arg_count}<{type_list}>(&self, {args})
                where {where_clause};
            "##,
            arg_count = type_params.len(),
            type_list = type_params.join(","),
            args = xform_types_i(&type_params, |i, x| format!("arg{}: {}", i, x)).join(","),
            where_clause =
                xform_types(&type_params, |x| format!("{t}: ProbeArgType<{t}>", t = x)).join(",")
        );
    }

    decl.push_str("}");

    decl
}

/// This generates the declaration of the `UnsafeProviderProbeNativeImpl` trait.  This trait provides an
/// implementation of `UnsafeProviderProbeImpl`, which invokes the wrapper code to convert all of the arguments
/// to their C native equivalent, and then passes that on to the specific implementator of `UnsafeProviderProbeNativeImpl`.
fn generate_unsafe_provider_probe_native_impl_trait() -> String {
    let mut decl= r#"
        /// See `UnsafeProviderProbeImpl` for additional details.  This subtrait provides an implementation of
        /// `UnsafeProviderProbeImpl` which wraps each of the Rust types into a `ProbeArgWrapper` and presents to the
        /// implementor of this trait the C representation of each probe argument.  It is presumbed that implmeentations of
        /// `UnsafeProviderProbeNativeImpl` are passing these parameters directly into C APIs.
        ///
        /// The implementor of this API for a specific tracing library need only implement all 13
        /// possible `fire` methods, one for each number of args from 0 to 12.
        ///
        /// *IMPORTANT NOTE TO IMPLEMENTORS*: Each of the `fireN` methods take arguments which may be either
        /// integers or possibly pointers to strings or other memory.  The caller guarantees that these are valid
        /// addresses *only* for the duration of the call.  Immediatley after the `fireN` method returns this memory may
        /// be freed.  Thus it's imperative that the probing implementation process probes synchronously.  Otherwise
        /// invalid memory accesses are inevitable.
        #[allow(clippy::too_many_arguments)]
        pub trait UnsafeProviderProbeNativeImpl
        {
            /// Tests if this probe is enabled or not.  This should be a very fast test, ideally just a memory
            /// access.  The Rust compiler should be able to inline this implementation for maxmimum performance.
            fn is_enabled(&self) -> bool;

            /// This is actually identical to `fire0` but is provided for consistency with the other arities
            unsafe fn c_fire0(&self);

    "#.to_string();

    for arity in 1..=MAX_ARITY {
        //For every possible arity level `N`, declare the probe method `c_fireN` which takes C native argument types
        let type_params = get_type_param_names(arity);

        decl += &format!(
            r##"
            unsafe fn c_fire{arg_count}<{type_list}>(&self, {args})
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

    decl += "}\n";

    //Above was the declaration of UnsafeProviderProbeNativeImpl.  Now provide a blanket implementation of
    //UnsafeProviderProbeImpl for all implementations of UnsafeProviderProbeNativeImpl which performs the conversion
    //from Rust to C types using `ProbeArgWrapper`.
    decl += r#"
        impl<T: UnsafeProviderProbeNativeImpl> UnsafeProviderProbeImpl for T
        {
            fn is_enabled(&self) -> bool {
                T::is_enabled(self)
            }
            unsafe fn fire0(&self) { self.c_fire0() }

    "#;

    for arity in 1..=MAX_ARITY {
        //For every possible arity level `N`, implement the `fireN` method declared in the parent trait,
        //by wrapping each arg in its `ProbeArgWrapper` wrapper and passing a native C representation of that
        //arg to the `c_fireN` method.
        let type_params = get_type_param_names(arity);
        let wrapper_decls = xform_types_i(&type_params, |i, _| {
            format!("let wrapper{0} = wrap(arg{0})", i)
        })
        .join(";\n");
        let probe_args =
            xform_types_i(&type_params, |i, _| format!("wrapper{}.as_c_type()", i)).join(",");

        decl += &format!(
            r##"
            unsafe fn fire{arg_count}<{type_list}>(&self, {args})
                where {where_clause} {{
                {wrapper_decls};
                self.c_fire{arg_count}({probe_args});
            }}
            "##,
            type_list = type_params.join(","),
            arg_count = type_params.len(),
            args = xform_types_i(&type_params, |i, x| format!("arg{}: {}", i, x)).join(","),
            where_clause =
                xform_types(&type_params, |x| format!("{t}: ProbeArgType<{t}>", t = x)).join(","),
            wrapper_decls = wrapper_decls,
            probe_args = probe_args,
        );
    }

    decl += "}\n";

    decl
}

/// This doesn't generate ALL test code, but it generates some test helpers that the `probes` module will use.
fn generate_tests() -> String {
    [
        generate_test_unsafe_probe_impl(),
        "\n".to_string(),
        generate_probe_tests(),
    ]
    .concat()
}

/// Generates a test-only implementation of UnsafeProviderProbeImpl which passes its parameters to `sprintf` and
/// exposes the resulting string as a Rust string type for verification
fn generate_test_unsafe_probe_impl() -> String {
    let mut decl = r#"

    #[cfg(test)]
    #[cfg(unix)]
    #[allow(clippy::too_many_arguments)]
    impl UnsafeProviderProbeNativeImpl for TestingProviderProbeImpl {
        fn is_enabled(&self) -> bool {
            self.is_enabled
        }

        unsafe fn c_fire0(&self) {
            {
                let buffer = self.buffer.lock().unwrap();
                libc::snprintf(buffer.as_ptr() as *mut c_char, BUFFER_SIZE, self.format_string.as_ptr());
            }

            self.log_call();
        }
    "#
    .to_string();

    for arity in 1..=MAX_ARITY {
        //For every possible arity level, declare a `probeN` method that takes a tuple of N
        //native (meaning C native) types to pass to the probe.
        let type_params = get_type_param_names(arity);

        decl += &format!(
            r##"
            unsafe fn c_fire{arg_count}<{type_list}>(&self, {args})
                where {where_clause} {{
                {{
                    let buffer = self.buffer.lock().unwrap();
                    libc::snprintf(buffer.as_ptr() as *mut c_char,
                        BUFFER_SIZE,
                        self.format_string.as_ptr(),
                        {probe_args});
                }}
                self.log_call();
            }}
            "##,
            arg_count = type_params.len(),
            type_list = type_params.join(","),
            args = xform_types_i(&type_params, |i, x| format!("arg{}: {}", i, x)).join(","),
            where_clause = xform_types(&type_params, |x| format!(
                "{t}: ProbeArgNativeType<{t}>",
                t = x
            ))
            .join(","),
            probe_args = if type_params.is_empty() {
                "".to_string()
            } else {
                xform_types_i(&type_params, |i, _| format!("arg{}", i)).join(",")
            }
        );
    }

    decl.push_str("}");

    decl
}

fn generate_probe_tests() -> String {
    const STRING_ARG_INDEX: usize = 4;
    const ARG_TYPES: &[(&str, &str)] = &[
        ("u64", "%u"),
        ("u32", "%u"),
        ("u16", "%u"),
        ("u8", "%u"),
        ("String", "%s"),
        ("bool", "%u"),
        ("i64", "%d"),
        ("i32", "%d"),
        ("i16", "%d"),
        ("i8", "%d"),
    ];

    /// Pick an argument type for argument `n`.
    ///
    /// Returns  tuple containing:
    /// * Name of the arg
    /// * Rust data type of the arg
    /// * C format string to use for the arg
    /// * bool indicating if this arg needs to be passed by reference to the probe
    fn choose_arg_for_n(n: usize) -> (String, String, String, bool) {
        let (type_name, format_specifier) = ARG_TYPES[n % ARG_TYPES.len()];

        let byref = type_name == "String" || type_name.starts_with("Option");

        (
            format!("arg{}", n),
            type_name.to_string(),
            format_specifier.to_string(),
            byref,
        )
    }

    let mut decl = "".to_string();

    for arity in 1..=MAX_ARITY {
        //For every possible arity level, write some quickcheck tests that take a tuple and exercise
        //the probe firing behavior.
        //
        //Note this test is made more complex by the fact that the max arity we support is currently 12, but
        //quickcheck doesn't support more than 8 parameters to a function under test.  So you'll see this code get
        //a bit creative; after 8 parameters, we introduce hard-coded strings as test values, so we can still exercise
        //all of the probing code up to the maximum arity
        let type_params = get_type_param_names(arity);
        let quickcheck_arg_count = if arity > MAX_QUICKCHECK_ARITY {
            MAX_QUICKCHECK_ARITY
        } else {
            arity
        };
        let additional_arg_count = arity - quickcheck_arg_count;

        //Quickcheck eligible args are generated the normal way, but after that we'll use strings
        //Note that we reverse this, to ensure that the last elements of a tuple are always the quickcheck
        //generated ones.  That ensures all 12 element positions get the full quickcheck exercise.
        let args = (1..=arity)
            .rev()
            .map(|n| {
                if n <= MAX_QUICKCHECK_ARITY {
                    choose_arg_for_n(n)
                } else {
                    let mut arg = choose_arg_for_n(STRING_ARG_INDEX);
                    arg.0 = format!("arg{}", n);
                    arg
                }
            })
            .collect::<Vec<(String, String, String, bool)>>();

        //The parameters to this function are only the ones we expect quickcheck to produce
        let args_declaration: Vec<String> = args
            .iter()
            .skip(additional_arg_count)
            .map(|(name, typ, _, _)| format!("{}: {}", name, typ))
            .collect();

        //To make up the difference, we'll declare some string locals ourselves
        let additional_args_declaration: Vec<String> = args
            .iter()
            .take(additional_arg_count)
            .map(|(name, _, _, _)| format!("let {name} = \"{name}\".to_string()", name = name))
            .collect();

        let expected_arg_values: Vec<String> = args
            .iter()
            .map(|(name, typ, _, _)| {
                if typ == "String" {
                    format!("c_and_back_again(&{})", name)
                } else if typ == "bool" {
                    format!("u8::from({})", name)
                } else {
                    name.to_string()
                }
            })
            .collect();

        let args_tuple = make_tuple_type(
            &args
                .iter()
                .map(|(name, _, _, byref)| {
                    if *byref {
                        format!("&{}", name)
                    } else {
                        format!("{}", name)
                    }
                })
                .collect(),
        );

        let c_format_string = args
            .iter()
            .map(|(_, _, fmt, _)| fmt.to_string())
            .collect::<Vec<String>>()
            .join(" ");

        let rust_format_string = std::iter::repeat("{}".to_string())
            .take(arity)
            .collect::<Vec<String>>()
            .join(" ");

        decl += &format!(
            r##"
            #[quickcheck]
            #[cfg(unix)]
            fn test_fire{arg_count}({args_declaration}) -> bool {{
                let unsafe_impl = TestingProviderProbeImpl::new("{c_format_string}".to_string());
                let probe_impl = ProviderProbe::new(&unsafe_impl);
                {additional_args_declaration};
                let probe_args={args_tuple};
                probe_impl.fire(probe_args);

                assert_eq!(probe_impl.unsafe_probe_impl.get_calls(),
                    vec![format!("{rust_format_string}", {expected_arg_values})]);
                true
            }}
            "##,
            arg_count = type_params.len(),
            args_declaration = args_declaration.join(", "),
            additional_args_declaration = additional_args_declaration.join(";\n"),
            c_format_string = c_format_string,
            args_tuple = args_tuple,
            rust_format_string = rust_format_string,
            expected_arg_values = expected_arg_values.join(", ")
        );
    }

    decl
}
