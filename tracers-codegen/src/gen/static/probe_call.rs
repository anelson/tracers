//! This generates probe calls for the `probe!` macro when the no-op implementation is being used.
//! That means the call doesn't do anything at runtime, but it should still include in the code an
//! unreachable line that performs the call, just to make sure the compiler still does type
//! checking and counts the arguments as being used.

use crate::build_rs::BuildInfo;
use crate::gen::common;
use crate::spec::ProbeCallSpecification;
use crate::{TracersResult, TracingTarget};
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;

pub(crate) fn generate_probe_call(
    build_info: &BuildInfo,
    call: ProbeCallSpecification,
) -> TracersResult<TokenStream> {
    //It's a bug to use this function to generate code for a dynamic implementation
    assert!(!build_info.implementation.is_dynamic());

    match call {
        ProbeCallSpecification::FireOnly(details) => {
            match build_info.implementation.tracing_target() {
                TracingTarget::Disabled => {
                    //When tracing is disabled there is no actual implementation, and each of the
                    //probe methods on the struct are empty.  However we still need to call them,
                    //because otherwise the compiler will warn about an unused method.  Since the
                    //probe methods are deliberately marked as `deprecated`, we'll also have to
                    //suppress the warning about calling a deprecated function
                    let call = details.call;
                    Ok(quote! {
                        if false {
                            #[allow(deprecated)]
                            #call
                        }
                    })
                }
                target @ TracingTarget::NoOp
                | target @ TracingTarget::Stap
                | target @ TracingTarget::Lttng => {
                    //There is a low-level wrapper function with the same name as the probe, in the
                    //impl module for the trait.
                    //Need to rewrite the path to the provider trait, replacing the trait with the
                    //name of its corresponding impl mod.  Then create wrappers for each of the
                    //arguments before passing them to the impl mod.
                    let mut mod_path = details.provider.clone();
                    let (provider, _) = mod_path
                        .segments
                        .pop()
                        .expect("provider path can't be empty")
                        .into_tuple();
                    let mod_name = syn::Ident::new(
                        &common::get_provider_impl_mod_name(&provider.ident),
                        provider.span(),
                    );
                    mod_path.segments.push(mod_name.into());

                    let probe = &details.probe;

                    let conditional_expression = if target == TracingTarget::NoOp {
                        //No-op always hard-codes the condition to `false`, which the compiler will
                        //optimize away to nothing
                        quote! { false }
                    } else {
                        //For a real impl the conditional is the semaphore for the probe
                        let semaphore_name = syn::Ident::new(
                            &format!("{}_semaphore", &details.probe.ident).to_uppercase(),
                            details.probe.span(),
                        );

                        //TODO: if the `unlikely` intrinsic is ever stabilized, use that here so
                        //the optimizer knows this will be false most of the time
                        quote! { unsafe { std::ptr::read_volatile(&#mod_path::#semaphore_name as *const u16) != 0 } }
                    };

                    //For each argument, which is some arbitrary Rust expression, generate a
                    //variable name which will be used to hold the wrapper for that argument
                    let args_with_var_names: Vec<_> = details
                        .args
                        .iter()
                        .enumerate()
                        .map(|(index, arg)| {
                            let span = arg.span();
                            let arg_name = syn::Ident::new(&format!("__tracer_arg{}", index), span);

                            (arg, arg_name)
                        })
                        .collect();

                    //Generate the `let` statement assigning those variables to the wrapped
                    //versions of each probe argument
                    let wrapped_var_names = common::generate_tuple(
                        args_with_var_names.iter().map(|(_, arg_name)| arg_name),
                    );
                    let arg_names: Vec<_> =
                        args_with_var_names.iter().map(|(arg, _)| arg).collect();

                    let probe_parameters: Vec<_> = args_with_var_names
                        .iter()
                        .map(|(_, arg_name)| {
                            quote! { #arg_name.as_c_type() }
                        })
                        .collect();

                    //If there are any arguments, wrap them in the ProbeArgWrapper using the helper
                    //function generated by the `tracer` proc macro
                    let wrap_statement = if !details.args.is_empty() {
                        let wrap_func = syn::Ident::new(
                            &format!("__{}_wrap", details.probe.ident),
                            details.probe.span(),
                        );

                        quote! { let (#(#wrapped_var_names),*) = #mod_path::#wrap_func(#(#arg_names),*); }
                    } else {
                        quote! {}
                    };

                    let unsafe_block = if target == TracingTarget::NoOp {
                        //No unsafe block is needed and using one just triggers a warning
                        quote! {}
                    } else {
                        //'real' impls call unsafe extern functions
                        quote! { unsafe }
                    };

                    let span = details.call.span();
                    Ok(quote_spanned! {span=>
                        {
                            use ::tracers::runtime::ProbeArgWrapper as _;

                            if #conditional_expression {
                                #wrap_statement

                                #unsafe_block { #mod_path::#probe(#(#probe_parameters),*); }
                            }
                        }
                    })
                }
            }
        }
        ProbeCallSpecification::FireWithCode { .. } => unimplemented!(),
    }
}
