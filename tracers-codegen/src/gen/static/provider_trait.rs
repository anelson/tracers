//! This module contains the code that transforms a trait with the `tracer` attribute into the
//! infrastructure to perform tracing using a static, platform-specific implementation.
//!
//! The generated code is about 90% identical across all possible implementations, so it's shared.
//! All static targets, including `noop`, as well as the special case `disabled` target, use this
//! module.  When there is target-specific logic, it is selected based on the `BuildInfo` in effect
//! at the time of the code generation
use crate::build_rs::BuildInfo;
use crate::gen::common::{ProbeGeneratorBase, ProviderTraitGeneratorBase};
use crate::gen::r#static::native_code::{self, ProcessedProviderTrait};
use crate::spec::{ProbeArgSpecification, ProbeSpecification, ProviderSpecification};
use crate::TracersResult;
use crate::TracingImplementation;
use crate::{TracingTarget, TracingType};
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use std::borrow::Cow;
use syn::spanned::Spanned;

pub(crate) struct ProviderTraitGenerator<'bi> {
    build_info: Cow<'bi, BuildInfo>,
    spec: ProviderSpecification,
    processed_provider: Option<ProcessedProviderTrait>,
    probes: Vec<ProbeGenerator>,
}

impl<'bi> ProviderTraitGeneratorBase for ProviderTraitGenerator<'bi> {
    fn spec(&self) -> &ProviderSpecification {
        &self.spec
    }

    fn build_info(&self) -> &BuildInfo {
        &self.build_info
    }
}

impl<'bi> ProviderTraitGenerator<'bi> {
    pub fn new(
        build_info: &'bi BuildInfo,
        spec: ProviderSpecification,
    ) -> ProviderTraitGenerator<'bi> {
        //This implementation is specific to static tracing (of which `disabled` is merely a
        //special case)
        assert!(!build_info.implementation.is_dynamic());

        let mut build_info = Cow::Borrowed(build_info);

        //Attempt to load the processed provider trait info for this trait.  That's the state
        //information left behind from `build.rs` telling us where to find the generated C wrapper
        //and the generated Rust bindings for that wrapper.  This isn't generated for all targets,
        //and if generation fails it shouldn't cause a compile error but rather it should cause us
        //to fall back to the NoOp generator for this provider
        let processed_provider = if build_info.implementation.tracing_target().is_enabled() {
            match native_code::get_processed_provider_info(&spec) {
                Err(e) => {
                    eprintln!("Warning: {}", e);

                    //This needs to override the implementation from whatever it was to noop
                    //because the code generation was unsuccessful
                    build_info.to_mut().implementation = TracingImplementation::StaticNoOp;

                    None
                }
                Ok(processed_provider) => Some(processed_provider),
            }
        } else {
            //Else the implementation isn't 'real' it's `Disabled` so no need to look for the
            //processed provider info
            None
        };

        //Consume this provider spec and separate out the probe specs, each of which we want to
        //wrap in our own ProbeGenerator
        let (spec, probes) = spec.separate_probes();
        let probes: Vec<_> = probes.into_iter().map(ProbeGenerator::new).collect();
        ProviderTraitGenerator {
            build_info,
            spec,
            processed_provider,
            probes,
        }
    }

    pub fn generate(&self) -> TracersResult<TokenStream> {
        // Re-generate this trait as a struct with our probing implementation in it
        let tracer_struct = self.generate_tracer_struct()?;

        // Generate a module which will contain the low-level implementation which actually
        // performs the tracing.
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
        for probe in self.probes.iter() {
            probe_methods.push(probe.generate_trait_methods(self)?);
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

        let trait_doc_comment = self.generate_trait_comment();

        let try_init_decl = self.generate_try_init_decl();

        //the __try_init_provider returns a Result.  In this static implementation, we'll hard-code
        //a successful result, with a string containing some metadata about the generated provider.
        //Only dynamic implementations can actually fail to initialize, which doesn't apply here
        let provider_name = self.spec.name();

        let implementation = match self.build_info.implementation.tracing_target() {
            TracingTarget::Disabled => TracingType::Disabled.as_ref().to_string(),
            TracingTarget::NoOp | TracingTarget::Stap => format!(
                "{}/{}",
                self.build_info.implementation.tracing_type().as_ref(),
                self.build_info.implementation.as_ref()
            ),
        };
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
        let span = self.spec.item_trait().span();
        let vis = &self.spec.item_trait().vis;
        let mod_name = self.get_provider_impl_mod_name();
        let native_declarations = self
            .probes
            .iter()
            .map(|p| p.generate_native_declaration(&self));

        match self.build_info.implementation.tracing_target() {
            TracingTarget::Disabled => {
                //When tracing is disabled we can't assume the `tracers::runtime` is available so
                //there is no implementation module in that case
                quote! {}
            }
            TracingTarget::NoOp => {
                // Generate a module which has dummy versions the functions that would have been
                // generated from C++ code in a real target.  These versions don't actually call
                // down into any C++ code of course, but their presence, and the implementation
                // calling them, verifies at compile time that the probe argument types all have
                // suitable `ProbeArgType` implementations so that if this is ever re-compiled to
                // support a real tracing back-end everything will work as expected
                //
                // When the target is `noop` the "native" implementations won't actually be Rust
                // FFI bindings, despite the name
                quote_spanned! {span=>
                    #vis mod #mod_name {
                        #(#native_declarations)*
                    }
                }
            }
            TracingTarget::Stap => {
                //The implementations which depend upon a generated C++ wrapper library work a bit
                //differently than `NoOp`.  The implementation mod will declare `extern` functions
                //for each wrapper function, and also `extern static` variables for each probe's
                //semaphore.  That's the dtrace/stap term for a 16 bit unsigned int that is
                //initially `0` and set to non-zero when a probe is enabled.  A critical part of
                //our high-performance design is the use of this semaphore to detect when a probe
                //is enabled with nothing more than a mem read.
                //
                //There is no impl struct for the real implementations
                let processed_provider = self
                    .processed_provider
                    .as_ref()
                    .expect("stap requires successful codegen");
                let lib_name = processed_provider
                    .lib_path
                    .file_stem()
                    .expect("expected valid lib file name")
                    .to_str()
                    .expect("lib file name is not a valid Rust string");

                quote_spanned! {span=>
                    #vis mod #mod_name {
                        #[link(name = #lib_name)]
                        extern "C" {
                            #(#native_declarations)*
                        }
                    }
                }
            }
        }
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

    pub fn generate_trait_methods(
        &self,
        provider: &ProviderTraitGenerator,
    ) -> TracersResult<TokenStream> {
        let vis = &self.spec.vis;
        let original_method = self.spec.original_method.sig.clone();

        let method_body = self.generate_probe_method_body(&provider)?;

        //Keep the original probe method, but mark it deprecated with a helpful message so that if the
        //user calls the probe method directly they will at least be reminded that they should use the
        //macro instead.
        let deprecation_attribute = self.generate_probe_deprecation_attribute(&provider.spec);

        //Keep any attributes that were on the original method, and add `doc` attributes at the end
        //to provide some more information about the generated probe mechanics
        let attrs = &self.spec.original_method.attrs;
        let probe_doc_comment = self.generate_probe_doc_comment(&provider.spec);

        let allow_attr = if provider.build_info.implementation.is_enabled() {
            //We will generate another probe method inside the impl module which is used to fire
            //the probe.  So in normal use this original method will never be called.  That will
            //confuse users because the `probe!` macro causes what looks like a methdo call on the
            //original probe function.  So put an attribute on the original function suppressing
            //that warning
            quote! { #[allow(dead_code)] }
        } else {
            //Tracing is disabled, so there is no impl mod, so the `probe!` calls will actually
            //reference the original method.  That means if a probe method is unused, we want the
            //compiler to warn the user about it just like it would any other unused method
            quote! {}
        };

        let span = original_method.span();
        Ok(quote_spanned! {span=>
            #(#attrs)*
            #probe_doc_comment
            #deprecation_attribute
            #allow_attr
            #vis #original_method {
                #method_body
            }
        })
    }

    fn generate_probe_method_body(
        &self,
        provider: &ProviderTraitGenerator,
    ) -> TracersResult<TokenStream> {
        let span = self.spec.original_method.span();
        // Generate the body of the original method.  This will have the same args as the trait
        // method declared by the caller, but we will provide an actual implementation.

        // * In the case of a `disabled` implementation, the function won't do anything at all.
        // We'll just assign all of the args in a `let _ = $ARGNAME` statement so that the compiler
        // doens't warn about unused arguments.
        //
        // * In the case of either a `noop` implementation or one of the 'real' implementations
        // with a C++ wrapper layer, we'll do the same thing the `probe!` macro does, and wrap each
        // of the args in the `ProbeArgType`-provided wrapper before passing them to the "native"
        // wrapper function ("native" in quotes because for `noop` it's actually just a do-nothing
        // Rust impl that has the same signature as a native function would).
        match provider.build_info.implementation.tracing_target() {
            TracingTarget::Disabled => {
                //Disabled.  Just make the arguments go away
                let args = self.spec.args.iter().map(|arg| {
                    let span = arg.syn_typ().span();
                    let arg_name = arg.ident();
                    quote_spanned! {span=>
                        let _ = #arg_name;
                    }
                });

                Ok(quote_spanned! {span=>
                    #(#args)*
                })
            }
            TracingTarget::NoOp | TracingTarget::Stap => {
                //This is a `real` impl with a C wrapper underneath (or in the case of `noop` a
                //Rust function with the same signature as a C wrapper).
                //The implementation is in the impl mod, with each probe as a function named the
                //same as the original probe method declaration, but taking as arguments the C
                //version of each parameter (although obviously declared as the Rust equivalent).
                //
                //Thus, there's no practical need for this method, other than to ensure if a user
                //mis-uses the probing library and tries to call the probe method directly, it
                //actually works (but they will still get a warning as this is not a very
                //performant way to fire probes)
                let mod_name = provider.get_provider_impl_mod_name();
                let probe_name = &self.spec.method_name;
                let wrap_args = self.spec.args.iter().map(|arg| {
                    let arg_name = arg.ident();

                    quote! { let #arg_name = ::tracers::runtime::wrap(#arg_name); }
                });
                let arg_names = self.spec.args.iter().map(ProbeArgSpecification::ident);

                Ok(quote_spanned! {span=>
                    // The compiler warns on this import as unused, even though without this trait
                    // imported the use of `as_c_type()` will fail
                    #[allow(unused_imports)]
                    use ::tracers::runtime::ProbeArgWrapper as _;

                    #(#wrap_args)*
                    #mod_name::#probe_name(#(#arg_names.as_c_type()),*);
                })
            }
        }
    }

    /// Generates the declaration for the "native" C++ functions which fire the probes using
    /// whatever the platform's tracing system is.  Depending upon the target, this generates one
    /// of two possible things:
    ///
    /// For the `NoOp` target, this generates Rust functions with the same signatures as the native
    /// functions would have been, but rather than being `extern` FFI declarations, these are
    /// actually implemented with an empty method body that does nothing.  This way the actual
    /// probe firing code generated by the `probe!` macro is the same for either `NoOp` or a real
    /// implementation.
    ///
    /// For real implementations (anything but `StaticNoOp` and `Disabled`), generates an `extern
    /// "C"` block which declares the native wrapper functions, which will be linked in a static
    /// library generated already at build time in `build.rs`.
    fn generate_native_declaration(&self, provider: &ProviderTraitGenerator) -> TokenStream {
        //Because of limitations in the tracing system, the name of the provider needs to
        //be fairly simple (no punctuation for example).  So we use the name of the trait,
        //converted to snake case.  Thus it's theoretically possible for there to be name
        //collisions.  That's why the name of the native library and the wrapper functions
        //are namespaced with a hash of the trait's source code, so if there is a
        //collision they will be disambiguated by the different implementation.  And, if
        //two crates happen to have the same exact provider trait declaration, then they'll
        //be treated as the same for tracing purposes.
        //
        //The only exception is the semaphore, because the C tracing macros make
        //assumptions about its name based on the provider and probe names.  Fortunately
        //even if there is a collission here, it won't result in any UB; it just means a
        //probe might think it's enabled when it's not, leading to a slightly inefficient
        //call into the wrapper function which will end up being a no-op
        assert!(provider.build_info.implementation != TracingImplementation::Disabled);
        let is_real = provider
            .build_info
            .implementation
            .tracing_target()
            .is_enabled();
        let provider_name = provider.spec.name();
        let provider_name_with_hash = provider.spec.name_with_hash();

        let native_func_name = format!("{}_{}", provider_name_with_hash, self.spec.name);
        let func_attrs = if is_real {
            quote! { #[link(name = #native_func_name)] }
        } else {
            quote! {}
        };
        let func_ident = &self.spec.method_name;

        let native_semaphore_name = format!("{}_{}_semaphore", provider_name, self.spec.name);
        let semaphore_name = format!("{}_semaphore", self.spec.name).to_uppercase();
        let semaphore_ident = syn::Ident::new(&semaphore_name, self.spec.original_method.span());
        let semaphore_attrs = if is_real {
            quote! {
               #[link(name = #native_semaphore_name)]
               #[link_section = ".probes"]
            }
        } else {
            quote! {}
        };

        let args = self.spec.args.iter().map(|arg| {
            let arg_name = arg.ident();
            let rust_typ: syn::Type = syn::parse_str(arg.arg_type_info().get_rust_type_str())
                .unwrap_or_else(|_| {
                    panic!(
                        "Failed to parse Rust type expression '{}'",
                        arg.arg_type_info().get_rust_type_str()
                    )
                });

            let span = arg.ident().span();
            quote_spanned! {span=>
                #arg_name: #rust_typ
            }
        });

        let func_body = if is_real {
            quote! { ; }
        } else {
            //The dummy no-op impl just pro-forma uses each argument to avoid a warning about
            //unused arguments
            let args_use = self.spec.args.iter().map(|arg| {
                let arg_name = arg.ident();

                let span = arg.ident().span();
                quote_spanned! {span=>
                    let _ = #arg_name;
                }
            });

            quote! {
                {
                    #(#args_use)*
                }
            }
        };

        let semaphore_initializer = if is_real {
            quote! { ; }
        } else {
            quote! { = 0; }
        };
        let span = self.spec.original_method.span();
        quote_spanned! {span=>
            #func_attrs
            pub fn  #func_ident( #(#args)* ) #func_body

            #semaphore_attrs
            pub static #semaphore_ident: u16 #semaphore_initializer
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
                TracingImplementation::Disabled,
                TracingImplementation::StaticNoOp,
            ]
            .into_iter()
            {
                let item_trait = test_case.get_item_trait();
                let spec = ProviderSpecification::from_trait(item_trait).expect(&format!(
                    "Failed to create specification from test trait '{}'",
                    test_case.description
                ));

                let build_info = BuildInfo::new(implementation);
                let generator = ProviderTraitGenerator::new(&build_info, spec);
                generator.generate().expect(&format!(
                    "Failed to generate test trait '{}'",
                    test_case.description
                ));
            }
        }
    }

    #[test]
    fn falls_back_to_disabled_on_error() {
        //If the native wrapper generation in `build.rs` failed, should fall back to `NoOp` no
        //matter what implementation was requested.  Since this test doesn't bother trying to
        //simulate the build-time code generation, it's guaranteed that there will be no
        //ProcessedProviderTrait for any of the provider traits, and thus the fallback logic should
        //be triggered
        for test_case in testdata::get_test_provider_traits(|c: &testdata::TestProviderTrait| {
            c.expected_error.is_none()
        })
        .into_iter()
        {
            for implementation in vec![TracingImplementation::StaticStap].into_iter() {
                let item_trait = test_case.get_item_trait();
                let spec = ProviderSpecification::from_trait(item_trait).expect(&format!(
                    "Failed to create specification from test trait '{}'",
                    test_case.description
                ));

                let build_info = BuildInfo::new(implementation);
                let generator = ProviderTraitGenerator::new(&build_info, spec);
                assert_eq!(
                    TracingImplementation::StaticNoOp,
                    generator.build_info.implementation
                );
                generator.generate().expect(&format!(
                    "Failed to generate test trait '{}'",
                    test_case.description
                ));
            }
        }
    }
}
