extern crate proc_macro;
//We have to use the `proc_macro` types for the actual macro impl, but everywhere else we'll use
//`proc_macro2` for better testability
use proc_macro::TokenStream as CompilerTokenStream;
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned, ToTokens};
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

    prober_impl(input).into()
}

struct ProberError {
    message: &'static str,
    span: Span,
}

impl ProberError {
    fn new(message: &'static str, span: Span) -> ProberError {
        ProberError { message, span }
    }
}

type ProberResult<T> = std::result::Result<T, ProberError>;

struct ProbeSpecification {
    name: String,
    method_name: Ident,
    original_method: TraitItemMethod,
    span: Span,
    args: Vec<FnArg>,
}

/// Actual implementation of the macro logic, factored out of the proc macro itself so that it's
/// more testable
fn prober_impl(item: ItemTrait) -> TokenStream {
    match generate_prober_trait(item) {
        Ok(stream) => stream,
        Err(err) => report_error(err.message, err.span),
    }
}

fn generate_prober_trait(item: ItemTrait) -> ProberResult<TokenStream> {
    if item.generics.type_params().next() != None || item.generics.lifetimes().next() != None {
        return Err(ProberError::new(
            "Probe traits must not take any lifetime or type parameters",
            item.span(),
        ));
    }

    // Look at the methods on the trait and translate each one into a probe specification
    let probes = get_probes(&item)?;

    // From the probe specifications, generate the corresponding methods that will be on the probe
    // trait.
    let mut probe_methods: Vec<TokenStream> = Vec::new();
    for probe in probes.iter() {
        probe_methods.push(generate_probe_trait_methods(probe)?);
    }

    // Re-generate the trait method that we took as input, with the modifications to support
    // probing
    let ident = item.ident;
    let vis = item.vis;

    let result = quote! {
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
                specs.push(get_probe(m)?);
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

/// Given a trait method, compute the probe that corresponds to that method.
/// If the method isn't suitable for use as a probe method, returns an error
///
/// Probe methods must have the following qualities:
/// * First argument must be `&self`
/// * Zero or more arguments after that; each argument must be a type that is supported by the
/// probe infrastructure (Note this requirement is enforced by the compiler not by this function)
/// * Return type of `()`
/// * No default implementation
/// * Not `unsafe`, `const`, `async`, or `extern "C"`
/// * No type parameters; generics are not supported in probe types
/// * Not variadic
fn get_probe(method: &TraitItemMethod) -> ProberResult<ProbeSpecification> {
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
    match first_arg {
        None => {
            return Err(ProberError::new(
                "Probe methods must take at least one argument, `&self`",
                method.span(),
            ));
        }
        Some(FnArg::SelfRef(selfarg)) => {
            if selfarg.mutability != None {
                return Err(ProberError::new(
                    "Probe methods must take `&self`, not `&mut self`",
                    method.span(),
                ));
            }
        }
        Some(_) => {
            return Err(ProberError::new(
                "The first argument of all probe methods must be `&self`",
                method.span(),
            ));
        }
    }

    Ok(ProbeSpecification {
        name: method.sig.ident.to_string(),
        method_name: method.sig.ident.clone(),
        original_method: method.clone(),
        span: method.span(),
        args: method.sig.decl.inputs.iter().map(|x| x.clone()).collect(),
    })
}

/// Each probe needs to have two methods generated on the probe trait: the original method written
/// by the user which describes the probe, and another one with an `_enabled` suffix which returns
/// a bool indicating if the probe is currently enabled or not.
fn generate_probe_trait_methods(probe: &ProbeSpecification) -> ProberResult<TokenStream> {
    let original_method = &probe.original_method;
    let mut enabled_method = original_method.clone();
    enabled_method.sig.ident = Ident::new(
        &format!("{}_enabled", probe.method_name),
        original_method.sig.ident.span(),
    );
    Ok(quote! {
        #original_method
        #enabled_method
    })
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
                    fn probe0(&self, arg0: i32);
                    fn probe1(&self, arg0: &str);
                    fn probe2(&self, arg0: &str, arg1: usize);
                }
            }
        }

        pub(crate) fn has_trait_type_param() -> ItemTrait {
            parse_quote! {
                trait TestTrait<T: Debug> {
                    fn probe0(&self, arg0: i32);
                    fn probe1(&self, arg0: &str);
                    fn probe2(&self, arg0: &str, arg1: usize);
                }
            }
        }

        pub(crate) fn has_const() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    fn probe0(&self, arg0: i32);
                    const FOO: usize = 5;
                }
            }
        }

        pub(crate) fn has_type() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    fn probe0(&self, arg0: i32);
                    type Foo = Debug;
                }
            }
        }

        pub(crate) fn has_macro_invocation() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    println!("WTF");

                    fn probe0(&self, arg0: i32);
                }
            }
        }

        pub(crate) fn has_const_fn() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    const fn probe0(&self, arg0: i32);
                }
            }
        }

        pub(crate) fn has_unsafe_fn() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    unsafe fn probe0(&self, arg0: i32);
                }
            }
        }

        pub(crate) fn has_extern_fn() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    extern "C" fn probe0(&self, arg0: i32);
                }
            }
        }

        pub(crate) fn has_fn_type_param() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    fn probe0<T: Debug>(&self, arg0: T);
                }
            }
        }

        pub(crate) fn has_explicit_unit_retval() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    fn probe0(&self, arg0: usize) -> ();
                }
            }
        }

        pub(crate) fn has_non_unit_retval() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    fn probe0(&self, arg0: usize) -> bool;
                }
            }
        }
        pub(crate) fn has_default_impl() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    fn probe0(&self, arg0: i32) { prinln!("{}", arg0); }
                }
            }
        }

        pub(crate) fn has_static_method() -> ItemTrait {
            parse_quote! {
                trait TestTrait {
                    fn probe0(arg0: i32);
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
        assert_eq!(true, generate_prober_trait(data::simple_valid()).is_ok());
    }

    #[test]
    fn trait_type_params_not_allowed() {
        // We need to be able to programmatically generate an impl of the probe trait which means
        // it cannot take any type parameters which we would not know how to provide
        assert_eq!(
            true,
            generate_prober_trait(data::has_trait_type_param()).is_err()
        );
    }

    #[test]
    fn non_method_items_not_allowed() {
        // A probe trait can't have anything other than methods.  That means no types, consts, etc
        assert_eq!(true, generate_prober_trait(data::has_const()).is_err());
        assert_eq!(true, generate_prober_trait(data::has_type()).is_err());
        assert_eq!(
            true,
            generate_prober_trait(data::has_macro_invocation()).is_err()
        );
    }

    #[test]
    fn method_modifiers_not_allowed() {
        // None of the Rust method modifiers like const, unsafe, async, or extern are allowed
        assert_eq!(true, generate_prober_trait(data::has_const_fn()).is_err());
        assert_eq!(true, generate_prober_trait(data::has_unsafe_fn()).is_err());
        assert_eq!(true, generate_prober_trait(data::has_extern_fn()).is_err());
    }

    #[test]
    fn generic_methods_not_allowed() {
        // Probe methods must not be generic
        assert_eq!(
            true,
            generate_prober_trait(data::has_fn_type_param()).is_err()
        );
    }

    #[test]
    fn method_retvals_not_allowed() {
        // Probe methods never return a value.  I would like to be able to support methods that
        // explicitly return `()`, but it wasn't immediately obvious how to do that with `syn` and
        // it's more convenient to declare probe methods without any return type anyway
        assert_eq!(
            true,
            generate_prober_trait(data::has_explicit_unit_retval()).is_err()
        );
        assert_eq!(
            true,
            generate_prober_trait(data::has_non_unit_retval()).is_err()
        );
    }

    #[test]
    fn methods_must_take_self_ref_non_mutable() {
        // Probe methods must all take `&self` and be non-mut
        assert_eq!(
            true,
            generate_prober_trait(data::has_static_method()).is_err()
        );
        assert_eq!(
            true,
            generate_prober_trait(data::has_mut_self_method()).is_err()
        );
        assert_eq!(
            true,
            generate_prober_trait(data::has_self_by_val_method()).is_err()
        );
    }

    #[test]
    fn methods_must_not_have_default_impl() {
        // The whole point of this macro is to generate implementations of the probe methods so ti
        // doesn't make sense for the caller to provide their own
        assert_eq!(
            true,
            generate_prober_trait(data::has_default_impl()).is_err()
        );
    }

}
