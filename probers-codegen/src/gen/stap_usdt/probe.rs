use crate::probe::ProbeSpecification;
use crate::provider::ProviderSpecification;
use crate::syn_helpers;
use crate::{ProberError, ProberResult};
use heck::{ShoutySnakeCase, SnakeCase};
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use std::borrow::BorrowMut;
use std::fmt::Display;
use syn::parse_quote;
use syn::spanned::Spanned;
use syn::{Ident, ItemTrait};

pub(super) struct ProbeGenerator<'spec> {
    spec: &'spec ProbeSpecification,
}

impl<'spec> ProbeGenerator<'spec> {
    pub fn new(spec: &'spec ProbeSpecification) -> ProbeGenerator<'spec> {
        ProbeGenerator { spec }
    }

    /// The name of the variable in the implementation struct which will hold this particular
    /// probe's `ProviderProbe` wrapper object
    pub(crate) fn probe_var_name(&self) -> &Ident {
        &self.spec.method_name
    }

    /// For each probe the user defines on the trait, we will generate multiple implementation
    /// methods:
    ///
    /// * `(probe_name)` - This is the same name as the method the user declared.  It takes the same
    /// arguments the user specified, and when called it checks to see if the probe is enabled and
    /// if so fires the probe.  This is implemented as a normal Rust function call, so the
    /// arguments to the probe function are evaluated unconditionally, whether the probe is enabled
    /// or not.
    /// * `(probe_name)_is_enabled` - This takes no args and returns a `bool` indicating if the
    /// probe is enabled or not.  Most situations won't require this method, but in some rare cases
    /// where some specific operation is conditional upon the enabling of a specific probe, this is
    /// available.
    /// * `if_(probe_name)_enabled` - This is a more complex version of `(probe_name)_is_enabled`
    /// which takes as an argument a `FnOnce` closure, which itself is passed a `FnOnce` closure
    /// which when called will fire the probe.  If the probe is not enabled, this closure never
    /// gets called.  Thus, in this way callers can implement potentially expensive logic to
    /// prepare information for a probe, and only run this code when the probe is activated.
    pub fn generate_trait_methods(
        &self,
        trait_name: &syn::Ident,
        provider_name: &str,
        struct_type_path: &syn::Path,
    ) -> ProberResult<TokenStream> {
        let vis = &self.spec.vis;
        //The original method will be implemented as a call to the impl method.  It's only purpose
        //is to ensure the user can call the original method and get our warning reminding them ot
        //use the `probe!` macro instead.  Otherwise it would be confusing to not be able to call a
        //method they think should exist on a trait they themselves defined, even if doing so is
        //not the intended use of this crate.
        let original_method = self.spec.original_method.sig.clone();

        //Generate an _enabled method which tests if this probe is enabled at runtime
        let mut enabled_method = original_method.clone();
        enabled_method.ident = syn_helpers::add_suffix_to_ident(&enabled_method.ident, "_enabled");
        enabled_method.decl.inputs = syn::punctuated::Punctuated::new();
        enabled_method.decl.output = syn::ReturnType::Default;

        //Generate an (probe)_probe method which returns the raw Option<ProviderProbe>
        let mut probe_method = original_method.clone();
        probe_method.ident = Ident::new(
            &format!("get_{}_probe", probe_method.ident),
            probe_method.ident.span(),
        );
        probe_method.decl.inputs = syn::punctuated::Punctuated::new();
        probe_method.decl.output = syn::ReturnType::Default;
        let probe_method_ret_type = self.generate_provider_probe_type();
        let a_lifetime = syn::Lifetime::new("'a", self.spec.span);
        probe_method
            .decl
            .generics
            .params
            .push(syn::GenericParam::Lifetime(syn::LifetimeDef::new(
                a_lifetime,
            )));
        for param in self.args_lifetime_parameters().iter() {
            probe_method
                .decl
                .generics
                .params
                .push(syn::GenericParam::Lifetime(syn::LifetimeDef::new(
                    param.clone(),
                )))
        }

        //Generate an _impl method that actually fires the probe when called
        let mut impl_method = original_method.clone();
        impl_method.ident = syn_helpers::add_suffix_to_ident(&impl_method.ident, "_impl");

        //Generate the body of the original method, simply passing its arguments directly to the
        //impl method
        let probe_args_tuple = self.args_as_tuple_value();

        //Keep the original probe method, but mark it deprecated with a helpful message so that if the
        //user calls the probe method directly they will at least be reminded that they should use the
        //macro instead.
        let probe_name = &self.spec.name;
        let probe_ident = &self.spec.method_name;
        let deprecation_message = format!( "Probe methods should not be called directly.  Use the `probe!` macro, e.g. `probe!({}::{}(...))`",
            trait_name,
            probe_name);

        //Keep any attributes that were on the original method, and add `doc` attributes at the end
        //to provide some more information about the generated probe mechanics
        let attrs = &self.spec.original_method.attrs;
        let probe_fire_comment = format!(
            r###"
To fire this probe, don't call this method directly. Instead, use the `probe!` macro, for example:

```ignore
// If the probe is enabled, fires the probe.  If the probe isn't enabled, or if provider
// initialization failed for some reason, does not fire the probe, and does NOT evaluate the
// arguments to the probe.
probe!({trait_name}::{probe_name}(...));
```
"###,
            trait_name = trait_name,
            probe_name = probe_name
        );
        let systemtap_comment = format!(
            r###"
To trace the firing of this probe, use `bpftrace`, e.g.:
```text
sudo bpftrace -p ${{PID}} -e 'usdt::{provider}:{probe} {{ printf("Hello from {probe}\n"); }}'
```

where `${{PID}}` should be the actual process ID of the process you are tracing.
"###,
            provider = provider_name,
            probe = probe_name
        );

        // Note that we don't put an #[allow(dead_code)] attribute on the original method, because
        // the user declared that method.  If it's not being used, let the compiler warn them about
        // it just like it would any other unused method.  The methods we generate, however, won't
        // be directly visible to the user and thus should not cause a warning if left un-called
        Ok(quote_spanned! { original_method.span() =>
                                                            #(#attrs)*
                #[doc = "# Probing

This method is translated at compile-time by `probers` into a platform-specific tracing
probe, which allows very high-performance and low-overhead tracing.

## How to fire probe

"]
                                        #[doc = #probe_fire_comment]
        #[doc = "
The exact details of how to interact with the probes depends on the underlying
probing implementation.

## SystemTap/USDT (Linux x64)
"]
                                        #[doc = #systemtap_comment]
        #[doc ="
## Other platforms

TODO: No other platforms supported yet
"]
                                                            #[deprecated(note = #deprecation_message)]
                                                            #[allow(dead_code)]
                                                            #vis #original_method {
                                                                if let Some(probes) = #struct_type_path::get() {
                                                                    if probes.#probe_ident.is_enabled() {
                                                                        probes.#probe_ident.fire(#probe_args_tuple)
                                                                    }
                                                                };
                                                            }

                                                            #[allow(dead_code)]
                                                            #[doc(hidden)]
                                                            #vis #enabled_method -> bool {
                                                                if let Some(probes) = #struct_type_path::get() {
                                                                    probes.#probe_ident.is_enabled()
                                                                } else {
                                                                    false
                                                                }
                                                            }

                                                            #[doc(hidden)]
                                                            #vis #probe_method -> Option<&'static #probe_method_ret_type> {
                                                                #struct_type_path::get().map(|probes| &probes.#probe_ident)
                                                            }
                                                        })
    }

    /// When building a provider, individual probes are added by calling `add_probe` on the
    /// `ProviderBuilder` implementation.  This method generates that call for this probe.  In this
    /// usage the lifetime parameters are not needed.
    pub(crate) fn generate_add_probe_call(&self, builder: &Ident) -> TokenStream {
        //The `add_probe` method takes one type parameter, which should be the tuple form of the
        //arguments for this probe.
        let args_type = self.args_as_tuple_type_with_lifetimes();
        let probe_name = &self.spec.name;

        quote_spanned! { self.spec.original_method.span() =>
            #builder.add_probe::<#args_type>(#probe_name)?;
        }
    }

    /// Each probe has a corresponding field in the struct that we build for the provider.  That
    /// field is an instance of `ProviderProbe` which is a type-safe wrapper around the underlying
    /// untyped implementation.  Because it's type safe it must necessarily have type parameters
    /// corresponding to the arguments to the probe.  Thus its declaration gets a bit complicated.
    ///
    /// Further complicating matters is that the lifetime elision that makes it so easy to declare
    /// functions with reference args isn't available here, so every reference parameter the probe
    /// takes needs to have a corresponding lifetime.  This gets messy, as you'll see.
    pub(crate) fn generate_provider_probe_type(&self) -> TokenStream {
        let arg_tuple = self.args_as_tuple_type_with_lifetimes();

        //In addition to the lifetime params for any ref args, all `ProviderProbe`s have a lifetime
        //param 'a which corresponds to the lifetime of the underlying `UnsafeProviderProbeImpl`
        //which they wrap.  That is the same for all probes, so we just hard-code it as 'a
        let a_lifetime = syn::Lifetime::new("'a", self.spec.span);

        quote_spanned! { self.spec.span =>
            ::probers::ProviderProbe<#a_lifetime, ::probers::SystemProbe, #arg_tuple>
        }
    }

    /// Generates the declaration of the member field within the provider implementation struct
    /// that holds the `ProviderProbe` instance for this probe.  It's a complex declaration because
    /// it must include lifetime parameters for all of the reference types used by any of this
    /// probe's arguments
    pub(crate) fn generate_struct_member_declaration(&self) -> TokenStream {
        let name = self.probe_var_name();
        let typ = self.generate_provider_probe_type();

        quote_spanned! { self.spec.span =>
            #name: #typ
        }
    }

    /// When we create a new instance of the struct which represents the provider and holds the
    /// `ProviderProbe` objects for all of the probes, each of those members needs to be
    /// initialized as part of the initialization expression for the struct.  This method generates
    /// the initialization expression for just this probe.
    ///
    /// For the whole struct it would look something like:
    ///
    /// ```noexecute
    /// FooProviderImpl{
    ///     probe1: provider.probe::<(i32,)>("probe1")?,
    ///     probe2: provider.probe::<(&str,&str,)>("probe2")?,
    ///     ...
    /// }
    /// ```
    ///
    /// This method generates just the line corresponding to this probe, without a trailing comma.
    pub(crate) fn generate_struct_member_initialization(&self, provider: &Ident) -> TokenStream {
        let name_literal = &self.spec.name;
        let name_ident = &self.spec.method_name;
        let args_tuple = self.args_as_tuple_type_without_lifetimes();

        quote_spanned! { self.spec.span =>
            #name_ident: #provider.get_probe::<#args_tuple>(#name_literal)?
        }
    }

    /// Gets all of the lifetime parameters for all of the reference args for this probe, in a
    /// `Vec` for convenient post-processing.
    ///
    /// For example:
    ///
    /// ```noexecute
    /// fn probe(arg0: &str, arg1: usize, arg2: Option<Result<(), &String>>;
    ///
    /// // results in vec!['probe_arg0_1, 'probe_arg2, _1]
    /// ```
    pub(crate) fn args_lifetime_parameters(&self) -> Vec<syn::Lifetime> {
        self.spec
            .args
            .iter()
            .map(|arg| arg.lifetimes())
            .flatten()
            .collect::<Vec<syn::Lifetime>>()
    }

    /// Build a tuple value expression, consisting of the names of the probe arguments in a tuple.
    /// For example:
    ///
    /// ```noexecute
    /// fn probe(arg0: &str, arg1: usize); //results in tuple: (arg0, arg1,)
    /// ```
    pub(crate) fn args_as_tuple_value(&self) -> TokenStream {
        let names = self.spec.args.iter().map(|arg| arg.ident());

        if self.spec.args.is_empty() {
            quote! { () }
        } else {
            quote_spanned! { self.spec.original_method.sig.decl.inputs.span() =>
                ( #(#names),* ,)
            }
        }
    }

    /// Build a tuple type expression whose elements correspond to the arguments of this probe.
    /// This includes only the type of each argument, and has no explicit lifetimes specified.  For
    /// that there is `args_as_tuple_type_with_lifetimes`
    pub(crate) fn args_as_tuple_type_without_lifetimes(&self) -> TokenStream {
        //When the probe spec is constructed lifetime parameters are added, so to construct a tuple
        //type without them they need to be stripped
        if self.spec.args.is_empty() {
            quote_spanned! { self.spec.span => () }
        } else {
            // Build alist of all of the arg types, but use the version without lifetimes
            let args = self.spec.args.iter().map(|arg| arg.syn_typ());

            //Now make a tuple type with the types
            quote_spanned! { self.spec.span =>
                ( #(#args),* ,)
            }
        }
    }

    /// Like the method above constructs a tuple type corresponding to the types of the arguments of this probe.
    ///  Unlike the above method, this tuple type is also annotated with explicit lifetime
    ///  parameters for all reference types in the tuple.
    pub(crate) fn args_as_tuple_type_with_lifetimes(&self) -> TokenStream {
        // same as the above method, but use the version with lifetime annotations
        let types = self
            .spec
            .args
            .iter()
            .map(|arg| arg.syn_typ_with_lifetimes());

        if self.spec.args.is_empty() {
            quote! { () }
        } else {
            quote_spanned! { self.spec.original_method.sig.decl.inputs.span() =>
                ( #(#types),* ,)
            }
        }
    }
}
