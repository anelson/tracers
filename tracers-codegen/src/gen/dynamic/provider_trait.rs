//!Code in this module processes the provider trait decorated with the `tracers` attribute, and
//!replaces it with an implementation using libstapsdt.
use crate::build_rs::BuildInfo;
use crate::gen::common::{ProbeGeneratorBase, ProviderTraitGeneratorBase};
use crate::spec::ProbeSpecification;
use crate::spec::ProviderSpecification;
use crate::syn_helpers;
use crate::TracersResult;
use heck::ShoutySnakeCase;
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::parse_quote;
use syn::spanned::Spanned;

pub(super) struct ProviderTraitGenerator<'bi> {
    build_info: &'bi BuildInfo,
    spec: ProviderSpecification,
    probes: Vec<ProbeGenerator>,
}

impl<'bi> ProviderTraitGeneratorBase for ProviderTraitGenerator<'bi> {
    fn spec(&self) -> &ProviderSpecification {
        &self.spec
    }

    fn build_info(&self) -> &BuildInfo {
        self.build_info
    }
}

impl<'bi> ProviderTraitGenerator<'bi> {
    pub fn new(
        build_info: &'bi BuildInfo,
        spec: ProviderSpecification,
    ) -> ProviderTraitGenerator<'bi> {
        //This implementation is specific to dynamic tracing
        assert!(build_info.implementation.is_dynamic());

        //Consume this provider spec and separate out the probe specs, each of which we want to
        //wrap in our own ProbeGenerator
        let (spec, probes) = spec.separate_probes();
        let probes: Vec<_> = probes.into_iter().map(ProbeGenerator::new).collect();
        ProviderTraitGenerator {
            build_info,
            spec,
            probes,
        }
    }

    pub fn generate(&self) -> TracersResult<TokenStream> {
        // Re-generate this trait as a struct with our probing implementation in it
        let tracer_struct = self.generate_tracer_struct()?;

        // Generate code for a struct and some `OnceCell` statics to hold the instance of the provider
        // and individual probe wrappers
        let impl_mod = self.generate_impl_mod();

        let span = self.spec.item_trait().span();
        Ok(quote_spanned! {span=>
            #tracer_struct

            #impl_mod
        })
    }
    /// A provider is described by the user as a `trait`, with methods corresponding to probes.
    /// However it's actually implemented as a `struct` with no member fields, with static methods
    /// implementing the probes.  Thus, given as input the `trait`, we produce a `struct` of the same
    /// name whose implementation actually performs the firing of the probes.
    fn generate_tracer_struct(&self) -> TracersResult<TokenStream> {
        // From the probe specifications, generate the corresponding methods that will be on the probe
        // struct.
        let mut probe_methods: Vec<TokenStream> = Vec::new();
        let mod_name = self.get_provider_impl_mod_name();
        let struct_type_name = self.get_provider_impl_struct_type_name();
        let struct_type_path: syn::Path = parse_quote! { #mod_name::#struct_type_name };
        for probe in self.probes.iter() {
            probe_methods.push(probe.generate_trait_methods(self, &struct_type_path)?);
        }

        // Re-generate the trait method that we took as input, with the modifications to support
        // probing
        // This includes constructing documentation for this trait, using whatever doc strings are already applied by
        // the user, plus a section of our own that has information about the provider and how it
        // translates into the various implementations.
        //
        // Hence, the rather awkward `#[doc...]` bits

        let attrs = &self.spec.item_trait().attrs;
        let span = self.spec.item_trait().span();
        let ident = &self.spec.item_trait().ident;
        let vis = &self.spec.item_trait().vis;

        let mod_name = self.get_provider_impl_mod_name();
        let struct_type_name = self.get_provider_impl_struct_type_name();
        let trait_doc_comment = self.generate_trait_comment();
        let try_init_decl = self.generate_try_init_decl();

        //the __try_init_provider returns a Result.  In this no-op implementation, we'll hard-code
        //a successful result, with a string containing some metadata about the generated provider
        let provider_name = self.spec.name();
        let implementation = format!(
            "{}/{}",
            self.build_info.implementation.tracing_type().as_ref(),
            self.build_info.implementation.as_ref()
        );

        let version = env!("CARGO_PKG_VERSION");

        let result = quote_spanned! {span=>
            #(#attrs)*
            #trait_doc_comment
            #vis struct #ident;

            impl #ident {
                #(#probe_methods)*

                #try_init_decl {
                    let result = #mod_name::#struct_type_name::get();

                    // On success, translate from the probe struct to the informational message
                    result.map(|_| {
                        concat!(#provider_name, "::", #implementation, "::", #version)
                    })
                }
            }
        };

        Ok(result)
    }

    /// The implementation of the probing logic is complex enough that it involves the declaration of a
    /// few variables and one new struct type.  All of this is contained within a module, to avoid the
    /// possibility of collissions with other code.  This method generates that module and all its
    /// contents.
    ///
    /// The contents are, briefly:
    /// * The module itself, named after the trait
    /// * A declaration of a `struct` which will hold references to all of the probes
    /// * Multiple static `OnceCell` variables which hold the underlying provider instance as well as
    /// the instance of the `struct` which holds references to all of the probes
    fn generate_impl_mod(&self) -> TokenStream {
        let mod_name = self.get_provider_impl_mod_name();
        let struct_type_name = self.get_provider_impl_struct_type_name();
        let struct_var_name = self.get_provider_impl_struct_var_name();
        let struct_type_params = self.generate_provider_struct_type_params();
        let instance_var_name = self.get_provider_instance_var_name();
        let define_provider_call = self.generate_define_provider_call();
        let provider_var_name = syn::Ident::new("p", self.spec.item_trait().span());
        let struct_members: Vec<_> = self
            .probes
            .iter()
            .map(ProbeGenerator::generate_struct_member_declaration)
            .collect();

        let struct_initializers: Vec<_> = self
            .probes
            .iter()
            .map(|probe| probe.generate_struct_member_initialization(&provider_var_name))
            .collect();

        let span = self.spec.item_trait().span();
        quote_spanned! {span=>
            mod #mod_name {
                use ::tracers::runtime::failure::{format_err, Fallible};
                use ::tracers::runtime::dynamic::once_cell::sync::OnceCell;
                use ::tracers::runtime::dynamic::{SystemTracer,SystemProvider,ProviderBuilder,Tracer};

                // Need the `Provider` trait in scope so we can access its methods on its
                // implementors
                use ::tracers::runtime::dynamic::Provider as _;
                use ::core::result::Result;

                #[allow(dead_code)]
                pub(super) struct #struct_type_name<#struct_type_params> {
                    #(pub #struct_members),*
                }

                unsafe impl<#struct_type_params> Send for #struct_type_name<#struct_type_params> {}
                unsafe impl<#struct_type_params> Sync for #struct_type_name <#struct_type_params>{}

                static #instance_var_name: OnceCell<Fallible<SystemProvider>> = OnceCell::new();
                static #struct_var_name: OnceCell<Result<#struct_type_name, String>> = OnceCell::new();
                static IMPL_OPT: OnceCell<Result<&'static #struct_type_name, &'static str>> = OnceCell::new();

                impl<#struct_type_params> #struct_type_name<#struct_type_params> {
                   #[allow(dead_code)]
                   pub(super) fn get() -> Result<&'static #struct_type_name<#struct_type_params>, &'static str> {
                       //let imp: &'static Result<&'static #struct_type_name, &'static str> = IMPL_OPT.get_or_init(|| {
                       let imp: &'static Result<_,_> = IMPL_OPT.get_or_init(|| {
                           // The reason for this seemingly-excessive nesting is that it's possible for
                           // both the creation of `SystemProvider` or the subsequent initialization of
                           // #struct_type_name to fail with different and also relevant errors.  By
                           // separting them this way we're able to preserve the details about any init
                           // failures that happen, while at runtime when firing probes it's a simple
                           // call of a method on an `Option<T>`.  I don't have any data to back this
                           // up but I suspect that allows for better optimizations, since we know an
                           // `Option<&T>` is implemented as a simple pointer where `None` is `NULL`.
                           let imp: &Result<#struct_type_name<#struct_type_params>, String> = #struct_var_name.get_or_init(|| {
                               // Initialzie the `SystemProvider`, capturing any initialization errors
                               let #provider_var_name: &Fallible<SystemProvider> = #instance_var_name.get_or_init(|| {
                                    #define_provider_call
                               });

                               // Transform this #provider_var_name into an owned `Fallible` containing
                               // references to `T` or `E`, since there's not much useful you can do
                               // with just a `&Result`.
                               match #provider_var_name.as_ref() {
                                   Err(e) => Err(format!("Provider initialization failed: {}", e)),
                                   Ok(#provider_var_name) => {
                                       // Proceed to create the struct containing each of the probes'
                                       // `ProviderProbe` instances
                                       Ok(
                                           #struct_type_name{
                                               #(#struct_initializers,)*
                                           }
                                       )
                                   }
                               }
                           });

                           //Convert this &Fallible<..> into an Result<&T, &'static str>
                           imp.as_ref().map_err(|e| e.as_ref())
                       });

                       //Copy this `&Result<&T, &String>` to a new `Result<&T, &str>`.  Since that should be
                       //implemented as just a pointer, this should be effectively free
                       //*imp
                       imp.map_err(|e| e.as_ref())
                   }
                }
            }
        }
    }

    /// A `Provider` is built by calling `define_provider` on a `Tracer` implementation.
    /// `define_provider` takes a closure and passes a `ProviderBuilder` parameter to that closure.
    /// This method generates the call to `SystemTracer::define_provider`, and includes code to add
    /// each of the probes to the provider
    fn generate_define_provider_call(&self) -> TokenStream {
        let builder = syn::Ident::new("builder", self.spec.item_trait().ident.span());
        let add_probe_calls: Vec<TokenStream> = self
            .probes
            .iter()
            .map(|probe| probe.generate_add_probe_call(&builder))
            .collect();
        let provider_name = self.spec.name();

        let span = self.spec.item_trait().span();
        quote_spanned! {span=>
            // The provider name must be chosen carefully.  As of this writing (2019-04) the `bpftrace`
            // and `bcc` tools have, shall we say, "evolving" support for USDT.  As of now, with the
            // latest git version of `bpftrace`, the provider name can't have dots or colons.  For now,
            // then, the provider name is just the name of the provider trait, converted into
            // snake_case for consistency with USDT naming conventions.  If two modules in the same
            // process have the same provider name, they will conflict and some unspecified `bad
            // things` will happen.
            let provider_name = #provider_name;

            SystemTracer::define_provider(&provider_name, |mut #builder| {
                #(#add_probe_calls)*

                Ok(builder)
            })
        }
    }

    /// The provider struct we declare to hold the probe objects needs to take a lot of type
    /// parameters.  One type, 'a, which corresponds to the lifetime parameter of the underling
    /// `ProviderProbe`s, and also one lifetime parameter for every reference argument of every probe
    /// method.
    ///
    /// The return value of this is a token stream consisting of all of the types, but not including
    /// the angle brackets.
    fn generate_provider_struct_type_params(&self) -> TokenStream {
        // Make a list of all of the reference param lifetimes of all the probes
        let probe_lifetimes: Vec<syn::Lifetime> = self
            .probes
            .iter()
            .map(ProbeGenerator::args_lifetime_parameters)
            .flatten()
            .collect();

        //The struct simply takes all of these lifetimes plus 'a
        quote! {
            'a, #(#probe_lifetimes),*
        }
    }

    /// The name of the static variable which contains the singleton instance of the provider struct,
    /// eg MYPROBESPROVIDERIMPL
    fn get_provider_impl_struct_var_name(&self) -> syn::Ident {
        syn::Ident::new(
            &format!("{}ProviderImpl", self.spec.item_trait().ident).to_shouty_snake_case(),
            self.spec.item_trait().span(),
        )
    }

    /// The name of the static variable which contains the singleton instance of the underlying tracing
    /// system's `Provider` instance, eg MYPROBESPROVIDER
    fn get_provider_instance_var_name(&self) -> syn::Ident {
        syn::Ident::new(
            &format!("{}Provider", self.spec.item_trait().ident).to_shouty_snake_case(),
            self.spec.item_trait().span(),
        )
    }
}

pub(super) struct ProbeGenerator {
    spec: ProbeSpecification,
}

impl ProbeGeneratorBase for ProbeGenerator {
    fn spec(&self) -> &ProbeSpecification {
        &self.spec
    }
}

impl ProbeGenerator {
    pub fn new(spec: ProbeSpecification) -> ProbeGenerator {
        ProbeGenerator { spec }
    }

    /// The name of the variable in the implementation struct which will hold this particular
    /// probe's `ProviderProbe` wrapper object
    pub(crate) fn probe_var_name(&self) -> &syn::Ident {
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
        provider: &ProviderTraitGenerator,
        struct_type_path: &syn::Path,
    ) -> TracersResult<TokenStream> {
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
        enabled_method.inputs = syn::punctuated::Punctuated::new();
        enabled_method.output = syn::ReturnType::Default;

        //Generate an get_(probe)_probe method which returns the raw Option<ProviderProbe>
        let mut probe_method = original_method.clone();
        probe_method.ident = syn::Ident::new(
            &format!("get_{}_probe", probe_method.ident),
            probe_method.ident.span(),
        );
        probe_method.inputs = syn::punctuated::Punctuated::new();
        probe_method.output = syn::ReturnType::Default;
        let probe_method_ret_type = self.generate_provider_probe_type();
        let a_lifetime = syn::Lifetime::new("'a", self.spec.span);
        probe_method
            .generics
            .params
            .push(syn::GenericParam::Lifetime(syn::LifetimeDef::new(
                a_lifetime,
            )));
        for param in self.args_lifetime_parameters().iter() {
            probe_method
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
        let probe_ident = &self.spec.method_name;
        let deprecation_attribute = self.generate_probe_deprecation_attribute(&provider.spec);

        //Keep any attributes that were on the original method, and add `doc` attributes at the end
        //to provide some more information about the generated probe mechanics
        let attrs = &self.spec.original_method.attrs;
        let probe_doc_comment = self.generate_probe_doc_comment(&provider.spec);

        // Note that we don't put an #[allow(dead_code)] attribute on the original method, because
        // the user declared that method.  If it's not being used, let the compiler warn them about
        // it just like it would any other unused method.  The methods we generate, however, won't
        // be directly visible to the user and thus should not cause a warning if left un-called
        let span = original_method.span();
        Ok(quote_spanned! {span=>
            #(#attrs)*

            #probe_doc_comment

            #deprecation_attribute
            #[allow(dead_code)]
            #vis #original_method {
                if let Ok(probes) = #struct_type_path::get() {
                    if probes.#probe_ident.is_enabled() {
                        probes.#probe_ident.fire(#probe_args_tuple)
                    }
                };
            }

            #[allow(dead_code)]
            #[doc(hidden)]
            #vis #enabled_method -> bool {
                if let Ok(probes) = #struct_type_path::get() {
                    probes.#probe_ident.is_enabled()
                } else {
                    false
                }
            }

            #[doc(hidden)]
            #vis #probe_method -> Option<&'static #probe_method_ret_type> {
                #struct_type_path::get().ok().map(|probes| &probes.#probe_ident)
            }
        })
    }

    /// When building a provider, individual probes are added by calling `add_probe` on the
    /// `ProviderBuilder` implementation.  This method generates that call for this probe.  In this
    /// usage the lifetime parameters are not needed.
    pub(crate) fn generate_add_probe_call(&self, builder: &syn::Ident) -> TokenStream {
        //The `add_probe` method takes one type parameter, which should be the tuple form of the
        //arguments for this probe.
        let args_type = self.args_as_tuple_type_with_lifetimes();
        let probe_name = &self.spec.name;

        let span = self.spec.original_method.span();
        quote_spanned! {span=>
            #builder.add_probe::<#args_type>(#probe_name)
                .map_err(|e| format_err!(concat!("Error adding probe '", #probe_name, "': {}"), e))?;
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

        let span = self.spec.span;
        quote_spanned! {span=>
            ::tracers::runtime::dynamic::ProviderProbe<#a_lifetime, ::tracers::runtime::dynamic::SystemProbe, #arg_tuple>
        }
    }

    /// Generates the declaration of the member field within the provider implementation struct
    /// that holds the `ProviderProbe` instance for this probe.  It's a complex declaration because
    /// it must include lifetime parameters for all of the reference types used by any of this
    /// probe's arguments
    pub(crate) fn generate_struct_member_declaration(&self) -> TokenStream {
        let name = self.probe_var_name();
        let typ = self.generate_provider_probe_type();

        let span = self.spec.span;
        quote_spanned! {span=>
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
    pub(crate) fn generate_struct_member_initialization(
        &self,
        provider: &syn::Ident,
    ) -> TokenStream {
        let name_literal = &self.spec.name;
        let name_ident = &self.spec.method_name;
        let args_tuple = self.args_as_tuple_type_without_lifetimes();

        let span = self.spec.span;
        quote_spanned! {span=>
            #name_ident: #provider.get_probe::<#args_tuple>(#name_literal)
                .map_err(|e| format!(concat!("Error getting probe '", #name_literal, "': {}"), e))?
        }
    }
}

/// It's quite difficult to meaningfully test code generators that use the `quote` crate.  These
/// tests exercise the code with various test cases, and verify that the generator doesn't fail or
/// panic.  But they do not verify that the generated code will compile.
///
/// The integration tests and examples in the `tracers` parent crate do that.
#[cfg(test)]
mod test {
    use super::*;
    use crate::testdata;
    use crate::TracingImplementation;

    #[test]
    fn generate_works_on_valid_traits() {
        for test_case in testdata::get_test_provider_traits(|c: &testdata::TestProviderTrait| {
            c.expected_error.is_none()
        })
        .into_iter()
        {
            for implementation in vec![
                TracingImplementation::DynamicNoOp,
                TracingImplementation::DynamicStap,
            ]
            .into_iter()
            {
                let (attr, item_trait) = test_case.get_attr_and_item_trait();
                let spec =
                    ProviderSpecification::from_trait(testdata::TEST_CRATE_NAME, attr, item_trait)
                        .expect(&format!(
                            "Failed to create specification from test trait '{}'",
                            test_case.description
                        ));

                let build_info =
                    BuildInfo::new(testdata::TEST_CRATE_NAME.to_owned(), implementation);
                let generator = ProviderTraitGenerator::new(&build_info, spec);
                generator.generate().expect(&format!(
                    "Failed to generate test trait '{}'",
                    test_case.description
                ));
            }
        }
    }
}
