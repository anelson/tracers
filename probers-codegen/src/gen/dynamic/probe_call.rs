//! This module generates the code for a call to a probe implemented with the `libstapsdt` library.
//! It's rather simple, because it assumes the Rust bindings on the `libstapsdt` API are already a
//! dependency and exposed via the `SystemTracer` type alias.

use crate::spec::ProbeCallSpecification;
use crate::ProberResult;
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;

/// Translates what looks to be an explicit call to the associated function corresponding to a
/// probe on a provider trait, into something which at runtime will most efficiently attempt to
/// access the global static instance of the probe and, if it's enabled, evaluate the args and fire
/// the probe.
///
/// It translates something like this:
///
/// ```noexecute
/// probe!(MyProvider::myprobe(1, 5, "this is a string", compute_something()));
/// ```
///
/// into:
///
/// ```noexecute
/// {
///     if let Some(probe) = MyProvider::get_myprobe_probe() {
///         if probe.is_enabled() {
///             probe.fire((1, 5, "this is a string", compute_something(),)));
///         }
///     }
/// }
/// ```
///
/// In particular, note that the probe's parameters are not evaluated unless the provider
/// initialized successfully and the probe is enabled.
pub(super) fn generate_probe_call(call: ProbeCallSpecification) -> ProberResult<TokenStream> {
    match call {
        ProbeCallSpecification::FireOnly(details) => {
            //Easy one.  This call is already set up like a Rust method call on the probe method of
            //the provider trait.  Just need to rewrite the name of the function from `(probename)`
            //to `get_(probename)_probe` and then make the call
            let probe_func_name = syn::Ident::new(
                &format!("get_{}_probe", details.probe.ident),
                details.probe.ident.span(),
            );
            let span = details.call.span();
            let provider = details.provider;

            //the `fire` method on the probe object takes a single argument, which is a tuple of
            //all of the probe args.  Build that here, though note there's a different syntax for
            //an empty tuple
            let args = if details.args.len() == 0 {
                quote! { () }
            } else {
                let args = details.args;
                quote! { (#(#args),*,) }
            };
            Ok(quote_spanned! {span=>
                {
                    if let Some(__probers_probe) = #provider::#probe_func_name() {
                        if __probers_probe.is_enabled() {
                            __probers_probe.fire(#(#args),*);
                        }
                    }
                }
            })
        }
        ProbeCallSpecification::FireWithCode { .. } => unimplemented!(),
    }
}
