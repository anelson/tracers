#![recursion_limit = "256"]

extern crate proc_macro;
//We have to use the `proc_macro` types for the actual macro impl, but everywhere else we'll use
//`proc_macro2` for better testability
use heck::{CamelCase, MixedCase, ShoutySnakeCase, SnakeCase};
use proc_macro::TokenStream as CompilerTokenStream;
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned, ToTokens};
use std::collections::HashMap;
use std::iter::FromIterator;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{
    braced, parse_macro_input, token, Field, FnArg, Ident, ItemTrait, ReturnType, Token, TraitItem,
    TraitItemMethod,
};

#[proc_macro_attribute]
pub fn prober(attr: CompilerTokenStream, item: CompilerTokenStream) -> CompilerTokenStream {
    // In our case this attribute can only be applied to a trait.  If it's not a trait, this line
    // will cause what looks to the user like a compile error complaining that it expected a trait.
    let input = parse_macro_input!(item as ItemTrait);

    match prober_impl(input) {
        Ok(stream) => stream,
        Err(err) => report_error(&err.message, err.span),
    }
    .into()
}

#[derive(Debug)]
struct ProberError {
    message: String,
    span: Span,
}

impl ProberError {
    fn new<M: ToString>(message: M, span: Span) -> ProberError {
        ProberError {
            message: message.to_string(),
            span: span,
        }
    }
}

type ProberResult<T> = std::result::Result<T, ProberError>;

struct ProbeSpecification {
    name: String,
    method_name: Ident,
    original_method: TraitItemMethod,
    span: Span,
    args: Vec<(syn::PatIdent, syn::Type)>,
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
    fn from_method(method: &TraitItemMethod) -> ProberResult<ProbeSpecification> {
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

        let mut args: Vec<(syn::PatIdent, syn::Type)> = Vec::new();
        for arg in method.sig.decl.inputs.iter() {
            if let FnArg::Captured(captured) = arg {
                if let syn::Pat::Ident(pat_ident) = &captured.pat {
                    args.push((pat_ident.clone(), captured.ty.clone()));
                    continue;
                }
            }
            return Err(ProberError::new(
                &format!("Probe method arguments should be in the form `name: TypeName`; {:?} is not an expected argument", arg),
                arg.span(),
            ));
        }

        Ok(ProbeSpecification {
            name: method.sig.ident.to_string(),
            method_name: method.sig.ident.clone(),
            original_method: method.clone(),
            span: method.span(),
            args: args,
        })
    }

    /// Each probe needs to have multiple methods generated on the probe trait: the original method written
    /// by the user which describes the probe, and another one with an `_enabled` suffix which returns
    /// a bool indicating if the probe is currently enabled or not.  This method generates the trait
    /// methods for this probe
    fn generate_trait_methods(&self, item: &ItemTrait) -> ProberResult<TokenStream> {
        let original_method = &self.original_method;

        //Generate an _enabled method which tests if this probe is enabled at runtime
        let mut enabled_method = original_method.clone();
        enabled_method.sig.ident = add_suffix_to_ident(&enabled_method.sig.ident, "_enabled");

        //Generate an _impl method that actually fires the probe when called
        let mut impl_method = original_method.clone();
        impl_method.sig.ident = add_suffix_to_ident(&impl_method.sig.ident, "_impl");

        //Keep the original probe method, but mark it deprecated with a helpful message so that if the
        //user calls the probe method directly they will at least be reminded that they should use the
        //macro instead.
        let trait_name = &item.ident;
        let probe_name = &self.name;
        let deprecation_message = format!( "Probe methods should not be called directly.  Use the `probe!` macro, e.g. `probe! {}::{}(...)`",
        trait_name,
        probe_name);

        println!("Original method: {}", quote! {#original_method});
        println!("Deprecation message: {}", deprecation_message);

        Ok(quote_spanned! { original_method.span() =>
            #[deprecated(note = #deprecation_message)]
            #original_method

            #enabled_method

            #impl_method
        })
    }

    /// When building a provider, individual probes are added by calling `add_probe` on the
    /// `ProviderBuilder` implementation.  This method generates that call for this probe
    fn generate_add_probe_call(&self, builder: &Ident) -> TokenStream {
        //The `add_probe` method takes one type parameter, which should be the tuple form of the
        //arguments for this probe.  It shouldn't be hard to create, just transform the arguments
        //of the probe method as we have them by removing the argument names, leaving just a
        //comma-separated list of types, then wrap in parentheses
        let args_type = self.get_args_as_tuple();
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
    fn generate_provider_probe_type(&self) -> TokenStream {
        let arg_tuple = self.get_args_as_tuple_with_lifetimes();

        //In addition to the lifetime params for any ref args, all `ProviderProbe`s have a lifetime
        //param 'a which corresponds to the lifetime of the underlying `UnsafeProviderProbeImpl`
        //which they wrap.  That is the same for all probes, so we just hard-code it as 'a
        let a_lifetime = syn::Lifetime::new("'a", self.span);

        quote_spanned! { self.span =>
            ProviderProbe<#a_lifetime, SystemProbe, #arg_tuple>
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
    ///     probe1: provider.get_probe::<(i32,)>("probe1")?,
    ///     probe2: provider.get_probe::<(&str,&str,)>("probe2")?,
    ///     ...
    /// }
    /// ```
    ///
    /// This method generates just the line corresponding to this probe, without a trailing comma.
    fn generate_struct_member_initialization(&self, provider: &Ident) -> TokenStream {
        let name_literal = &self.name;
        let name_ident = &self.method_name;
        let args_tuple = self.get_args_as_tuple();

        quote_spanned! { self.span =>
            #name_ident: #provider.get_probe::<#args_tuple>(#name_literal)?
        }
    }

    /// Scans the list of probe function arguments, returning all of the ones that are reference
    /// arguments.  We need to know that because each of the ref args will need a separate lifetime
    /// parameter on the `ProviderProbe` object.
    ///
    /// Lifetime parameter names are constructed to be unique within a given trait.  So for the
    /// following probe trait:
    ///
    /// ```noexecute
    /// trait MyProbe {
    ///     fn probe0();        //get_ref_args returns empty
    ///     fn probe1(x: usize);//get_ref_args returns empty
    ///     fn probe2(a: &str, b: &str); //['probe2_a, 'probe2_b]
    ///     fn probe3(a: &str); //['probe3_a]
    /// }
    /// ```
    fn get_ref_args(&self) -> Vec<(syn::PatIdent, syn::TypeReference, syn::Lifetime)> {
        self.args
            .iter()
            .filter_map(|(nam, typ)| {
                if let syn::Type::Reference(tr) = typ {
                    let lifetime = syn::Lifetime::new(
                        &format!("'{}_{}", self.method_name, nam.ident),
                        nam.span(),
                    );
                    Some((nam.clone(), tr.clone(), lifetime))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Build a tuple type expression whose elements correspond to the arguments of this probe.
    /// This includes only the type of each argument, and has no explicit lifetimes specified.  For
    /// that there is `get_args_as_tuple_with_lifetimes`
    fn get_args_as_tuple(&self) -> TokenStream {
        let types = self.args.iter().map(|(nam, typ)| typ);

        if self.args.is_empty() {
            quote! { () }
        } else {
            quote_spanned! { self.original_method.sig.decl.inputs.span() =>
                ( #(#types),* ,)
            }
        }
    }

    /// Like the method above constructs a tuple type corresponding to the arguments of this probe.
    ///  Unlike the above method, this tuple type is also annotated with explicit lifetime
    ///  parameters for all reference types in the tuple.
    fn get_args_as_tuple_with_lifetimes(&self) -> TokenStream {
        if self.args.is_empty() {
            quote_spanned! { self.span => () }
        } else {
            // Make a map keyed by the argument name, where the value is the type of the argument but
            // modified so the optional `lifetime` member is set
            let ref_types: HashMap<syn::PatIdent, syn::TypeReference> =
                HashMap::from_iter(self.get_ref_args().iter().map(|(nam, typ, lifetime)| {
                    let mut new_typ = typ.clone();

                    new_typ.lifetime = Some(lifetime.clone());

                    (nam.clone(), new_typ)
                }));

            //transform the list of types so that if a given parameter has a reference type, we use the
            //modified type that includes a lifetime specifier.  Otherwise use the type as it was.
            let args: Vec<syn::Type> = self
                .args
                .iter()
                .map(|(nam, typ)| match ref_types.get(nam) {
                    None => typ.clone(),
                    Some(ref_type) => syn::Type::Reference(ref_type.clone()),
                })
                .collect();

            //Now make a tuple type with the types
            quote_spanned! { self.span =>
                ( #(#args),* ,)
            }
        }
    }
}

/// Actual implementation of the macro logic, factored out of the proc macro itself so that it's
/// more testable
fn prober_impl(item: ItemTrait) -> ProberResult<TokenStream> {
    if item.generics.type_params().next() != None || item.generics.lifetimes().next() != None {
        return Err(ProberError::new(
            "Probe traits must not take any lifetime or type parameters",
            item.span(),
        ));
    }

    // Look at the methods on the trait and translate each one into a probe specification
    let probes = get_probes(&item)?;

    // Re-generate this trait with our probing implementation in it
    let probe_trait = generate_prober_trait(&item, &probes)?;

    // Generate code for a struct and some `OnceCell` statics to hold the instance of the provider
    // and individual probe wrappers
    let provider_struct = generate_provider_struct(&item, &probes);

    Ok(quote_spanned! { item.span() =>
        #probe_trait

        #provider_struct
    })
}

fn generate_prober_trait(
    item: &ItemTrait,
    probes: &Vec<ProbeSpecification>,
) -> ProberResult<TokenStream> {
    // From the probe specifications, generate the corresponding methods that will be on the probe
    // trait.
    let mut probe_methods: Vec<TokenStream> = Vec::new();
    for probe in probes.iter() {
        probe_methods.push(probe.generate_trait_methods(item)?);
    }

    // Re-generate the trait method that we took as input, with the modifications to support
    // probing
    let span = item.span();
    let ident = &item.ident;
    let vis = &item.vis;

    let result = quote_spanned! { span =>
        #vis trait #ident  {
            #(#probe_methods)*
        }
    };

    Ok(result)
}

/// Looking at the methods defined on the trait, deduce from those methods the probes that we will
/// need to define, including their arg counts and arg types.
///
/// If the trait contains anything other than method declarations, or any of the declarations are
/// not suitable as probes, an error is returned
fn get_probes(item: &ItemTrait) -> ProberResult<Vec<ProbeSpecification>> {
    let mut specs: Vec<ProbeSpecification> = Vec::new();
    for f in item.items.iter() {
        match f {
            TraitItem::Method(ref m) => {
                specs.push(ProbeSpecification::from_method(m)?);
            }
            _ => {
                return Err(ProberError::new(
                    "Probe traits must consist entirely of methods, no other contents",
                    f.span(),
                ));
            }
        }
    }

    Ok(specs)
}

/// Our implementation requires that we declare a `struct` type, named `xProvider_` where `x` is
/// the name of the probe trait .  This struct will be stored in a static `OnceCell` so it will be
/// initialized lazily on first use. This method also generates the code that performs this lazy
/// initialization
fn generate_provider_struct(item: &ItemTrait, probes: &Vec<ProbeSpecification>) -> TokenStream {
    let mod_name = get_provider_impl_mod_name(&item.ident);
    let struct_type_name = get_provider_struct_type_name(&item.ident);
    let struct_var_name = get_provider_struct_var_name(&item.ident);
    let struct_type_params = get_provider_struct_type_params(probes);
    let instance_var_name = get_provider_instance_var_name(&item.ident);
    let define_provider_call = generate_define_provider_call(&item, probes);
    let vis = &item.vis;
    let provider_var_name = syn::Ident::new("p", item.span());
    let struct_members: Vec<_> = probes
        .iter()
        .map(|probe| {
            let name = &probe.method_name;
            let typ = probe.generate_provider_probe_type();

            println!("Probe {}: {}: {}", probe.name, name, typ);

            quote_spanned! { probe.span =>
                #name: #typ
            }
        })
        .collect();

    let struct_initializers: Vec<_> = probes
        .iter()
        .map(|probe| probe.generate_struct_member_initialization(&provider_var_name))
        .collect();

    quote_spanned! { item.span() =>
        mod #mod_name {
            use ::failure::{bail, Fallible};
            use ::probers::{SystemTracer,SystemProvider,SystemProbe,ProviderProbe,Provider};
            use ::probers_core::{ProviderBuilder,Tracer,ProbeArgs};
            use ::once_cell::sync::OnceCell;

            #vis struct #struct_type_name<#struct_type_params> {
                #(#struct_members),*
            }

            unsafe impl<#struct_type_params> Send for #struct_type_name<#struct_type_params> {}
            unsafe impl<#struct_type_params> Sync for #struct_type_name <#struct_type_params>{}

            static #instance_var_name: OnceCell<Fallible<SystemProvider>> = OnceCell::INIT;
            static #struct_var_name: OnceCell<Fallible<#struct_type_name>> = OnceCell::INIT;
            static impl_opt: OnceCell<Option<&'static #struct_type_name>> = OnceCell::INIT;

            impl<#struct_type_params> #struct_type_name<#struct_type_params> {
               fn get() -> Option<&'static #struct_type_name<#struct_type_params>> {
                   let imp: &'static Option<&'static #struct_type_name> = impl_opt.get_or_init(|| {
                       // The reason for this seemingly-excessive nesting is that it's possible for
                       // both the creation of `SystemProvider` or the subsequent initialization of
                       // #struct_type_name to fail with different and also relevant errors.  By
                       // separting them this way we're able to preserve the details about any init
                       // failures that happen, while at runtime when firing probes it's a simple
                       // call of a method on an `Option<T>`.  I don't have any data to back this
                       // up but I suspect that allows for better optimizations, since we know an
                       // `Option<&T>` is implemented as a simple pointer where `None` is `NULL`.
                       let imp = #struct_var_name.get_or_init(|| {
                           // Initialzie the `SystemProvider`, capturing any initialization errors
                           let #provider_var_name: &Fallible<SystemProvider> = #instance_var_name.get_or_init(|| {
                                #define_provider_call
                           });

                           // Transform this #provider_var_name into an owned `Fallible` containing
                           // references to `T` or `E`, since there's not much useful you can do
                           // with just a `&Result`.
                           let #provider_var_name = #provider_var_name.as_ref();

                           match #provider_var_name {
                               Err(e) => bail!("Provider initialization failed: {}", e),
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

                       //Convert this &Fallible<..> into an Option<&T>
                       imp.as_ref().ok()
                   });

                   //Copy this `&Option<&T>` to a new `Option<&T>`.  Since that should be
                   //implemented as just a pointer, this should be effectively free
                   *imp
               }

               fn get_init_error() -> Option<&'static failure::Error> {
                    //Don't do a whole re-init cycle again, but if the initialization has happened,
                    //check for failure
                    #struct_var_name.get().and_then(|fallible|  fallible.as_ref().err() )
               }
            }
        }
    }
}

/// A `Provider` is built by calling `define_provider` on a `Tracer` implementation.
/// `define_provider` takes a closure and passes a `ProviderBuilder` parameter to that closure.
/// This method generates the call to `SystemTracer::define_provider`, and includes code to add
/// each of the probes to the provider
fn generate_define_provider_call(
    item: &ItemTrait,
    probes: &Vec<ProbeSpecification>,
) -> TokenStream {
    let provider_name = item.ident.to_string();
    let builder = Ident::new("builder", item.ident.span());
    let add_probe_calls: Vec<TokenStream> = probes
        .iter()
        .map(|probe| probe.generate_add_probe_call(&builder))
        .collect();

    quote_spanned! { item.span() =>
        SystemTracer::define_provider(module_path!(), |mut #builder| {
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
fn get_provider_struct_type_params(probes: &Vec<ProbeSpecification>) -> TokenStream {
    // Make a list of all of the reference param lifetimes of all the probes
    let probe_lifetimes: Vec<_> = probes
        .iter()
        .map(|p| {
            p.get_ref_args()
                .iter()
                .map(|(nam, typ, lifetime)| lifetime.clone())
                .collect::<Vec<syn::Lifetime>>()
        })
        .flatten()
        .collect();

    //The struct simply takes all of these lifetimes plus 'a
    quote! {
        'a, #(#probe_lifetimes),*
    }
}

/// Returns the name of the module in which most of the implementation code for this trait will be
/// located.
fn get_provider_impl_mod_name(trait_name: &Ident) -> Ident {
    Ident::new(
        &format!("{}Impl", trait_name).to_snake_case(),
        trait_name.span(),
    )
}

/// The name of the struct type which represents the provider, eg `MyProbesProviderImpl`
fn get_provider_struct_type_name(trait_name: &Ident) -> Ident {
    add_suffix_to_ident(trait_name, "ProviderImpl")
}

/// The name of the static variable which contains the singleton instance of the provider struct,
/// eg MYPROBESPROVIDERIMPL
fn get_provider_struct_var_name(trait_name: &Ident) -> Ident {
    Ident::new(
        &format!("{}ProviderImpl", trait_name).to_shouty_snake_case(),
        trait_name.span(),
    )
}

/// The name of the static variable which contains the singleton instance of the underlying tracing
/// system's `Provider` instance, eg MYPROBESPROVIDER
fn get_provider_instance_var_name(trait_name: &Ident) -> Ident {
    Ident::new(
        &format!("{}Provider", trait_name).to_shouty_snake_case(),
        trait_name.span(),
    )
}

/// Helper method which takes as input an `Ident` which represents a variable or type name, appends
/// a given suffix to that name, and returns it as a new `Ident`
fn add_suffix_to_ident(ident: &Ident, suffix: &str) -> Ident {
    Ident::new(&format!("{}{}", ident, suffix), ident.span())
}

/// Reports a compile error in our macro, which is then reported to the user via the
/// `compile_error!` macro injected into the token stream.  Cool idea stolen from
/// https://internals.rust-lang.org/t/custom-error-diagnostics-with-procedural-macros-on-almost-stable-rust/8113
fn report_error(msg: &str, span: Span) -> TokenStream {
    //NB: When the unstable feature `proc_macro_diagnostic` is stabilized, use that instead of this
    //hack
    //
    //span.unwrap().error(msg).emit();
    //TokenStream::new()
    quote_spanned! { span =>
        compile_error! { #msg }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use quote::quote;

    mod data {
        use quote::quote;
        use syn::{parse_quote, ItemTrait};

        pub(crate) fn simple_valid() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    fn probe0(arg0: i32);
                    fn probe1(arg0: &str);
                    fn probe2(arg0: &str, arg1: usize);
                }
            }
        }

        pub(crate) fn valid_with_many_refs() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    fn probe0(arg0: i32);
                    fn probe1(arg0: &str);
                    fn probe2(arg0: &str, arg1: usize);
                    fn probe3(arg0: &str, arg1: &usize, arg2: &Option<i32>);
                }
            }
        }

        pub(crate) fn has_trait_type_param() -> ItemTrait {
            parse_quote! {
                trait TestTrait<T: Debug> {
                    fn probe0(arg0: i32);
                    fn probe1(arg0: &str);
                    fn probe2(arg0: &str, arg1: usize);
                }
            }
        }

        pub(crate) fn has_const() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    fn probe0(arg0: i32);
                    const FOO: usize = 5;
                }
            }
        }

        pub(crate) fn has_type() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    fn probe0(arg0: i32);
                    type Foo = Debug;
                }
            }
        }

        pub(crate) fn has_macro_invocation() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    println!("WTF");

                    fn probe0(arg0: i32);
                }
            }
        }

        pub(crate) fn has_const_fn() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    const fn probe0(arg0: i32);
                }
            }
        }

        pub(crate) fn has_unsafe_fn() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    unsafe fn probe0(arg0: i32);
                }
            }
        }

        pub(crate) fn has_extern_fn() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    extern "C" fn probe0(arg0: i32);
                }
            }
        }

        pub(crate) fn has_fn_type_param() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    fn probe0<T: Debug>(arg0: T);
                }
            }
        }

        pub(crate) fn has_explicit_unit_retval() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    fn probe0(arg0: usize) -> ();
                }
            }
        }

        pub(crate) fn has_non_unit_retval() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    fn probe0(arg0: usize) -> bool;
                }
            }
        }
        pub(crate) fn has_default_impl() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    fn probe0(arg0: i32) { prinln!("{}", arg0); }
                }
            }
        }

        pub(crate) fn has_non_static_method() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    fn probe0(&self, arg0: i32);
                }
            }
        }

        pub(crate) fn has_mut_self_method() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    fn probe0(&mut self, arg0: i32);
                }
            }
        }

        pub(crate) fn has_self_by_val_method() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    fn probe0(self, arg0: i32);
                }
            }
        }

    }

    #[test]
    fn works_with_valid_cases() {
        assert_eq!(true, prober_impl(data::simple_valid()).is_ok());
    }

    #[test]
    fn trait_type_params_not_allowed() {
        // We need to be able to programmatically generate an impl of the probe trait which means
        // it cannot take any type parameters which we would not know how to provide
        assert_eq!(true, prober_impl(data::has_trait_type_param()).is_err());
    }

    #[test]
    fn non_method_items_not_allowed() {
        // A probe trait can't have anything other than methods.  That means no types, consts, etc
        assert_eq!(true, prober_impl(data::has_const()).is_err());
        assert_eq!(true, prober_impl(data::has_type()).is_err());
        assert_eq!(true, prober_impl(data::has_macro_invocation()).is_err());
    }

    #[test]
    fn method_modifiers_not_allowed() {
        // None of the Rust method modifiers like const, unsafe, async, or extern are allowed
        assert_eq!(true, prober_impl(data::has_const_fn()).is_err());
        assert_eq!(true, prober_impl(data::has_unsafe_fn()).is_err());
        assert_eq!(true, prober_impl(data::has_extern_fn()).is_err());
    }

    #[test]
    fn generic_methods_not_allowed() {
        // Probe methods must not be generic
        assert_eq!(true, prober_impl(data::has_fn_type_param()).is_err());
    }

    #[test]
    fn method_retvals_not_allowed() {
        // Probe methods never return a value.  I would like to be able to support methods that
        // explicitly return `()`, but it wasn't immediately obvious how to do that with `syn` and
        // it's more convenient to declare probe methods without any return type anyway
        assert_eq!(true, prober_impl(data::has_explicit_unit_retval()).is_err());
        assert_eq!(true, prober_impl(data::has_non_unit_retval()).is_err());
    }

    #[test]
    fn methods_must_not_take_self() {
        // Probe methods should not take a `self` parameter
        assert_eq!(true, prober_impl(data::has_non_static_method()).is_err());
        assert_eq!(true, prober_impl(data::has_mut_self_method()).is_err());
        assert_eq!(true, prober_impl(data::has_self_by_val_method()).is_err());
    }

    #[test]
    fn methods_must_not_have_default_impl() {
        // The whole point of this macro is to generate implementations of the probe methods so ti
        // doesn't make sense for the caller to provide their own
        assert_eq!(true, prober_impl(data::has_default_impl()).is_err());
    }

    #[test]
    fn probe_lifetime_params_unique() {
        let probe_trait = data::valid_with_many_refs();
        let probes = get_probes(&probe_trait).expect("This is a known valid trait");

        //This particular trait has four probes, see the valid_with_many_refs method for details
        let all_ref_args: Vec<_> = probes
            .iter()
            .map(|p| p.get_ref_args())
            .flatten()
            .map(|(_, _, lifetime)| lifetime.to_string())
            .collect();

        assert_eq!(
            all_ref_args,
            vec![
                "'probe1_arg0",
                "'probe2_arg0",
                "'probe3_arg0",
                "'probe3_arg1",
                "'probe3_arg2"
            ]
        );
    }

    #[test]
    fn probe_generate_args_as_tuple_with_lifetimes() {
        let probe_trait = data::valid_with_many_refs();
        let probes = get_probes(&probe_trait).expect("This is a known valid trait");

        //This particular trait has four probes, see the valid_with_many_refs method for details
        let all_ref_args: Vec<_> = probes
            .iter()
            .map(|p| p.get_args_as_tuple_with_lifetimes())
            .map(|tokenstream| tokenstream.to_string())
            .collect();

        assert_eq!(
            all_ref_args,
            vec![
                "( i32 , )",
                "( & 'probe1_arg0 str , )",
                "( & 'probe2_arg0 str , usize , )",
                "( & 'probe3_arg0 str , & 'probe3_arg1 usize , & 'probe3_arg2 Option < i32 > , )",
            ]
        );
    }
}
