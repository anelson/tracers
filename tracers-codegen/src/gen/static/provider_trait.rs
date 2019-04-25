//! This module contains the code that transforms a trait with the `tracer` attribute into the
//! infrastructure to perform tracing using a static, platform-specific implementation.
//!
//! The generated code is about 90% identical across all possible implementations, so it's shared.
//! All static targets, including `noop`, as well as the special case `disabled` target, use this
//! module.  When there is target-specific logic, it is selected based on the `BuildInfo` in effect
//! at the time of the code generation
use crate::build_rs::BuildInfo;
use crate::gen::common;
use crate::spec::ProbeSpecification;
use crate::spec::ProviderSpecification;
use crate::TracersResult;
use heck::SnakeCase;
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::parse_quote;
use syn::spanned::Spanned;

pub(crate) struct ProviderTraitGenerator<'bi> {
    build_info: &'bi BuildInfo,
    spec: ProviderSpecification,
    probes: Vec<ProbeGenerator<'bi>>,
}

impl<'bi> ProviderTraitGenerator<'bi> {
    pub fn new(
        build_info: &'bi BuildInfo,
        spec: ProviderSpecification,
    ) -> ProviderTraitGenerator<'bi> {
        //Consume this provider spec and separate out the probe specs, each of which we want to
        //wrap in our own ProbeGenerator
        let (spec, probes) = spec.separate_probes();
        let probes: Vec<_> = probes
            .into_iter()
            .map(|probe| ProbeGenerator::new(build_info, probe))
            .collect();
        ProviderTraitGenerator {
            build_info,
            spec,
            probes,
        }
    }

    pub fn generate(&self) -> TracersResult<TokenStream> {
        // Re-generate this trait as a struct with our probing implementation in it
        let tracer_struct = self.generate_tracer_struct()?;

        // Generate a module which will `use` all of the `ProbeArgType` impls and compile-time
        // verify all probe arg types have a suitable implementaiton
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
        let mod_name = self.get_provider_impl_mod_name();
        let struct_type_name = syn::Ident::new("ProbeArgTypeCheck", self.spec.item_trait().span());
        let struct_type_path: syn::Path = parse_quote! { #mod_name::#struct_type_name };
        let mut probe_methods: Vec<TokenStream> = Vec::new();
        for probe in self.probes.iter() {
            probe_methods.push(probe.generate_trait_methods(self, &struct_type_path)?);
        }

        // Re-generate the trait method that we took as input, with the modifications to support
        // probing
        // This includes constructing documentation for this trait, using whatever doc strings are already applied by
        // the user, plus a section of our own that has information about the provider and how it
        // translates into the various implementations.
        let attrs = &self.spec.item_trait().attrs;
        let span = self.spec.item_trait().span();
        let ident = &self.spec.item_trait().ident;
        let vis = &self.spec.item_trait().vis;

        let trait_doc_comment = common::generate_trait_comment(&self.spec);

        let try_init_decl = common::generate_try_init_decl(&self.spec);

        //the __try_init_provider returns a Result.  In this no-op implementation, we'll hard-code
        //a successful result, with a string containing some metadata about the generated provider
        let provider_name = self.spec.name();
        let implementation = format!("static/{}", self.build_info.implementation.as_ref());
        let version = env!("CARGO_PKG_VERSION");

        let result = quote_spanned! {span=>
            #(#attrs)*
            #trait_doc_comment
            #vis struct #ident;

            impl #ident {
                #(#probe_methods)*

                #try_init_decl {
                    Ok(concat!(#provider_name, "::", #implementation, "::", #version))
                }
            }
        };

        Ok(result)
    }

    fn generate_impl_mod(&self) -> TokenStream {
        if self.build_info.implementation.is_enabled() {
            //Generate a module that has some code to use our `ProbeArgType` trait to verify at
            //compile time that every probe argument has a corresponding C representation.
            //Since that requires that the `tracers` runtime be available to the caller, it won't
            //work if that runtime is missing
            let mod_name = self.get_provider_impl_mod_name();
            let struct_type_name =
                syn::Ident::new("ProbeArgTypeCheck", self.spec.item_trait().span());

            let span = self.spec.item_trait().span();
            quote_spanned! {span=>
                mod #mod_name {
                    use tracers::runtime::ProbeArgType;

                    pub(super) struct #struct_type_name<T: ProbeArgType<T>> {
                        _t: ::std::marker::PhantomData<T>,
                    }

                    impl<T: ProbeArgType<T>> #struct_type_name<T> {
                        #[allow(dead_code)]
                        pub fn wrap(arg: T) -> <T as ProbeArgType<T>>::WrapperType {
                            ::tracers::runtime::wrap::<T>(arg)
                        }
                    }
                }
            }
        } else {
            quote! {}
        }
    }

    /// Returns the name of the module in which most of the implementation code for this trait will be
    /// located.
    fn get_provider_impl_mod_name(&self) -> syn::Ident {
        let snake_case_name = format!("{}Provider", self.spec.item_trait().ident).to_snake_case();

        syn::Ident::new(
            &format!("__{}", snake_case_name),
            self.spec.item_trait().ident.span(),
        )
    }
}

pub(super) struct ProbeGenerator<'bi> {
    build_info: &'bi BuildInfo,
    spec: ProbeSpecification,
}

impl<'bi> ProbeGenerator<'bi> {
    pub fn new(build_info: &'bi BuildInfo, spec: ProbeSpecification) -> ProbeGenerator<'bi> {
        ProbeGenerator { build_info, spec }
    }

    pub fn generate_trait_methods(
        &self,
        provider: &ProviderTraitGenerator,
        struct_type_path: &syn::Path,
    ) -> TracersResult<TokenStream> {
        let vis = &self.spec.vis;
        let original_method = self.spec.original_method.sig.clone();

        //Generate the body of the original method, simply passing its arguments directly to the
        //type assertion method
        let args_type_assertions = self.spec.args.iter().map(|arg| {
            let span = arg.syn_typ().span();
            let arg_name = arg.ident();

            //If the runtime is available, then the type assertion method is available
            //If not, just generate an expression that will count as 'using' the argument so the
            //compiler doesn't complain
            if self.build_info.implementation.is_enabled() {
                quote_spanned! {span=>
                    #struct_type_path::wrap(#arg_name);
                }
            } else {
                quote_spanned! {span=>
                    let _ = #arg_name;
                }
            }
        });

        //Keep the original probe method, but mark it deprecated with a helpful message so that if the
        //user calls the probe method directly they will at least be reminded that they should use the
        //macro instead.
        let deprecation_attribute =
            common::generate_probe_deprecation_attribute(&provider.spec, &self.spec);

        //Keep any attributes that were on the original method, and add `doc` attributes at the end
        //to provide some more information about the generated probe mechanics
        let attrs = &self.spec.original_method.attrs;
        let probe_doc_comment = common::generate_probe_doc_comment(&provider.spec, &self.spec);

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
                if false {
                    #(#args_type_assertions)*
                }
            }
        })
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
            for is_enabled in [false, true].into_iter() {
                let item_trait = test_case.get_item_trait();
                let spec = ProviderSpecification::from_trait(&item_trait).expect(&format!(
                    "Failed to create specification from test trait '{}'",
                    test_case.description
                ));

                let build_info = if *is_enabled {
                    BuildInfo::new(TracingImplementation::StaticNoOp)
                } else {
                    BuildInfo::new(TracingImplementation::Disabled)
                };
                let generator = ProviderTraitGenerator::new(&build_info, spec);
                generator.generate().expect(&format!(
                    "Failed to generate test trait '{}'",
                    test_case.description
                ));
            }
        }
    }
}
