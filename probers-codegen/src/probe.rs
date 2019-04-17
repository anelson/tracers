//! This module declares and implements the `ProbeSpecification` struct, which represents a single
//! tracing probe corresponding to a single method on the trait which is marked with a `#[prober]`
//! attribute.
//!
//! This module encapsulates most of the messy details of translating a simple trait method into
//! the definition of a probe.

use crate::argtypes::{from_syn_type, ArgTypeInfo};
use crate::probe_arg::ProbeArgSpecification;
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use std::fmt;
use syn::spanned::Spanned;
use syn::Visibility;
use syn::{FnArg, Ident, ItemTrait, ReturnType, TraitItemMethod};

use super::syn_helpers;
use super::{ProberError, ProberResult};

pub(crate) struct ProbeSpecification {
    pub name: String,
    method_name: Ident,
    original_method: TraitItemMethod,
    vis: Visibility,
    span: Span,
    pub args: Vec<ProbeArgSpecification>,
}

impl fmt::Debug for ProbeSpecification {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ProbeSpecification(name={}, args=(", self.name)?;

        for arg in self.args.iter() {
            write!(f, "{:?}\n", arg)?;
        }

        write!(f, ")")
    }
}

impl ProbeSpecification {
    /// Given a trait method, compute the probe that corresponds to that method.
    /// If the method isn't suitable for use as a probe method, returns an error
    ///
    /// Probe methods must have the following qualities:
    /// * Must be static; no `&self` or `&mut self` or `self` first argument
    /// * Zero or more arguments after that; each argument must be a type that is supported by the
    /// probe infrastructure (Note this requirement is enforced by the compiler not by this function)
    /// * Return type of `()`
    /// * No default implementation
    /// * Not `unsafe`, `const`, `async`, or `extern "C"`
    /// * No type parameters; generics are not supported in probe types
    /// * Not variadic
    pub(crate) fn from_method(
        item: &ItemTrait,
        method: &TraitItemMethod,
    ) -> ProberResult<ProbeSpecification> {
        if method.default != None {
            return Err(ProberError::new(
                "Probe methods must NOT have a default implementation",
                method.span(),
            ));
        } else if method.sig.constness != None
            || method.sig.unsafety != None
            || method.sig.asyncness != None
            || method.sig.abi != None
        {
            return Err(ProberError::new(
                "Probe methods cannot be `const`, `unsafe`, `async`, or `extern \"C\"`",
                method.span(),
            ));
        } else if method.sig.decl.generics.type_params().next() != None {
            return Err(ProberError::new(
            "Probe methods must not take any type parameters; generics are not supported in probes",
            method.span(),
        ));
        } else if method.sig.decl.variadic != None {
            return Err(ProberError::new(
                "Probe methods cannot have variadic arguments",
                method.span(),
            ));
        } else if method.sig.decl.output != ReturnType::Default {
            return Err(ProberError::new(
                "Probe methods must not have an explicit return type (they return `()` implicitly)",
                method.span(),
            ));
        };

        let first_arg = method.sig.decl.inputs.iter().next();
        if let Some(FnArg::SelfRef(_)) = first_arg {
            return Err(ProberError::new(
                "Probe methods must not have any `&self` args",
                method.span(),
            ));
        } else if let Some(FnArg::SelfValue(_)) = first_arg {
            return Err(ProberError::new(
                "Probe methods must not have any `self` args",
                method.span(),
            ));
        }

        let mut args: Vec<ProbeArgSpecification> = Vec::new();
        for (idx, arg) in method.sig.decl.inputs.iter().enumerate() {
            args.push(ProbeArgSpecification::from_fnarg(method, idx, arg)?);
        }

        let spec = ProbeSpecification {
            name: method.sig.ident.to_string(),
            method_name: method.sig.ident.clone(),
            original_method: method.clone(),
            vis: item.vis.clone(),
            span: method.span(),
            args: args,
        };

        Ok(spec)
    }

    /// The name of the variable in the implementation struct which will hold this particular
    /// probe's `ProviderProbe` wrapper object
    pub(crate) fn probe_var_name(&self) -> &Ident {
        &self.method_name
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
    pub(crate) fn generate_trait_methods(
        &self,
        trait_name: &syn::Ident,
        provider_name: &str,
        struct_type_path: &syn::Path,
    ) -> ProberResult<TokenStream> {
        let vis = &self.vis;
        //The original method will be implemented as a call to the impl method.  It's only purpose
        //is to ensure the user can call the original method and get our warning reminding them ot
        //use the `probe!` macro instead.  Otherwise it would be confusing to not be able to call a
        //method they think should exist on a trait they themselves defined, even if doing so is
        //not the intended use of this crate.
        let original_method = self.original_method.sig.clone();

        //Generate an _enabled method which tests if this probe is enabled at runtime
        let mut enabled_method = original_method.clone();
        enabled_method.ident = syn_helpers::add_suffix_to_ident(&enabled_method.ident, "_enabled");
        enabled_method.decl.inputs = syn::punctuated::Punctuated::new();
        enabled_method.decl.output = syn::ReturnType::Default;

        //Generate an (probe)_probe method which returns the raw Option<ProviderProbe>
        let mut probe_method = original_method.clone();
        probe_method.ident = Ident::new(
            &format!("{}_probe", probe_method.ident),
            probe_method.ident.span(),
        );
        probe_method.decl.inputs = syn::punctuated::Punctuated::new();
        probe_method.decl.output = syn::ReturnType::Default;
        let probe_method_ret_type = self.generate_provider_probe_type();
        let a_lifetime = syn::Lifetime::new("'a", self.span);
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
        let probe_name = &self.name;
        let probe_ident = &self.method_name;
        let deprecation_message = format!( "Probe methods should not be called directly.  Use the `probe!` macro, e.g. `probe!({}::{}(...))`",
            trait_name,
            probe_name);

        //Keep any attributes that were on the original method, and add `doc` attributes at the end
        //to provide some more information about the generated probe mechanics
        let attrs = &self.original_method.attrs;
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
        let probe_name = &self.name;

        quote_spanned! { self.original_method.span() =>
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
        let a_lifetime = syn::Lifetime::new("'a", self.span);

        quote_spanned! { self.span =>
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

        quote_spanned! { self.span =>
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
        let name_literal = &self.name;
        let name_ident = &self.method_name;
        let args_tuple = self.args_as_tuple_type_without_lifetimes();

        quote_spanned! { self.span =>
            #name_ident: #provider.probe::<#args_tuple>(#name_literal)?
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
        self.args
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
        let names = self.args.iter().map(|arg| arg.ident());

        if self.args.is_empty() {
            quote! { () }
        } else {
            quote_spanned! { self.original_method.sig.decl.inputs.span() =>
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
        if self.args.is_empty() {
            quote_spanned! { self.span => () }
        } else {
            // Build alist of all of the arg types, but use the version without lifetimes
            let args = self.args.iter().map(|arg| arg.syn_typ());

            //Now make a tuple type with the types
            quote_spanned! { self.span =>
                ( #(#args),* ,)
            }
        }
    }

    /// Like the method above constructs a tuple type corresponding to the types of the arguments of this probe.
    ///  Unlike the above method, this tuple type is also annotated with explicit lifetime
    ///  parameters for all reference types in the tuple.
    pub(crate) fn args_as_tuple_type_with_lifetimes(&self) -> TokenStream {
        // same as the above method, but use the version with lifetime annotations
        let types = self.args.iter().map(|arg| arg.syn_typ_with_lifetime());

        if self.args.is_empty() {
            quote! { () }
        } else {
            quote_spanned! { self.original_method.sig.decl.inputs.span() =>
                ( #(#types),* ,)
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::testdata::*;
    use quote::quote;
    use syn::{parse_quote, TraitItemMethod};

    /// Allow test cases to match probe specifications against other probe specs
    impl PartialEq<ProbeSpecification> for ProbeSpecification {
        fn eq(&self, other: &ProbeSpecification) -> bool {
            //All members except Span support this already
            self.name == other.name
                && self.method_name == other.method_name
                && self.original_method == other.original_method
                && self.vis == other.vis
                && self.args == other.args
        }
    }

    /// Allows tests to compare test data directly to ProbeSpecification instances
    impl PartialEq<TestProbe> for ProbeSpecification {
        fn eq(&self, other: &TestProbe) -> bool {
            //All members except Span support this already
            self.name == other.name
                && self.args.len() == other.args.len()
                && self
                    .args
                    .iter()
                    .zip(other.args.iter())
                    .all(|(self_arg, spec_arg)| {
                        let (self_arg_name, self_arg_type) = (self_arg.ident(), self_arg.syn_typ());
                        let (spec_arg_name, spec_arg_type, _) = spec_arg;

                        self_arg_name.ident.to_string() == *spec_arg_name
                            && self_arg_type == spec_arg_type
                    })
        }
    }

    mod data {
        use super::*;

        pub(crate) fn trait_item() -> ItemTrait {
            parse_quote! { trait SomeTrait {} }
        }

        /// Produces an assortment of valid probe methods, all of which should be accepted by the
        /// `from_method` constructor
        pub(crate) fn valid_test_cases() -> Vec<TraitItemMethod> {
            vec![
                parse_quote! { fn probe0(arg0: i32); },
                parse_quote! { fn probe1(arg0: &str); },
                parse_quote! { fn probe2(arg0: &str, arg1: usize); },
                parse_quote! { fn probe3(arg0: &str, arg1: &usize, arg2: &Option<i32>); },
            ]
        }

        /// Produces an assortment of invalid probe methods, all of which should be rejected by the
        /// `from_method` constructor for various reasons
        pub(crate) fn invalid_test_cases() -> Vec<TraitItemMethod> {
            vec![
                parse_quote! { const fn probe0(arg0: i32); },
                parse_quote! { unsafe fn probe0(arg0: i32); },
                parse_quote! { extern "C" fn probe0(arg0: i32); },
                parse_quote! { fn probe0<T: Debug>(arg0: T); },
                parse_quote! { fn probe0(arg0: usize) -> (); },
                parse_quote! { fn probe0(arg0: usize) -> bool; },
                parse_quote! { fn probe0(arg0: i32) { prinln!("{}", arg0); } },
                parse_quote! { fn probe0(&self, arg0: i32); },
                parse_quote! { fn probe0(&mut self, arg0: i32); },
                parse_quote! { fn probe0(self, arg0: i32); },
            ]
        }

    }

    #[test]
    fn works_with_valid_cases() {
        for input in data::valid_test_cases().iter() {
            let input_string = quote! { #input }.to_string();

            ProbeSpecification::from_method(&data::trait_item(), input).expect(&format!(
                "This should be treated as a valid method: {}",
                input_string
            ));
        }
    }

    #[test]
    fn works_with_invalid_cases() {
        for input in data::invalid_test_cases().iter() {
            let input_string = quote! { #input }.to_string();

            ProbeSpecification::from_method(&data::trait_item(), input)
                .err()
                .expect(&format!(
                    "This should be treated as an invalid method: {}",
                    input_string
                ));
        }
    }

    #[test]
    fn decorates_args_with_lifetime_params() {
        // Verify that when a probe is created from a trait method, all reference types anywhere in
        // the probe method args are decorated with a unique explicit lifetime.  That's needed for
        // the cases in the generated impl code where we can't take advantage of lifetime elision.
        let test_cases: Vec<(syn::TraitItemMethod, proc_macro2::TokenStream)> = vec![
            (
                parse_quote! { fn probe0(arg0: i32); },
                quote! { fn probe0(arg0: i32); },
            ),
            (
                parse_quote! {fn probe1(arg0: &str);},
                quote! {fn probe1(arg0: &'probe1_arg0_1 str);},
            ),
            (
                parse_quote! { fn probe2(arg0: &str, arg1: usize); },
                quote! { fn probe2(arg0: &'probe2_arg0_1 str, arg1: usize); },
            ),
            (
                parse_quote! { fn probe2(arg0: &str, arg1: usize); },
                quote! { fn probe2(arg0: &'probe2_arg0_1 str, arg1: usize); },
            ),
            (
                parse_quote! { fn probe3(arg0: &str, arg1: &usize, arg2: &Option<&str>); },
                quote! { fn probe3(arg0: &'probe3_arg0_1 str, arg1: &'probe3_arg1_1 usize, arg2: &'probe3_arg2_1 Option<&'probe3_arg2_2 str>); },
            ),
        ];

        for (method, expected) in test_cases.iter() {
            let probe =
                ProbeSpecification::from_method(&data::trait_item(), method).expect(&format!(
                    "This method should be valid: {}",
                    syn_helpers::convert_to_string(method)
                ));

            //Re-construct the probe method using the args as they've been computed in the
            //ProbeSpecification.  The lifetimes should be present
            let args = probe.args.iter().map(|arg| {
                let (nam, typ) = (&arg.ident(), &arg.syn_typ_with_lifetime());
                quote! { #nam: #typ }
            });

            let name = probe.method_name;

            let result = quote! {
                fn #name(#(#args),*);
            };

            assert_eq!(expected.to_string(), result.to_string());
        }
    }
}
