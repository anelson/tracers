//! This module provides functionality to scan the AST of a Rust source file and identify
//! `probe-rs` provider traits therein, as well as analyze those traits and produce `ProbeSpec`s for
//! each of the probes they contain.  Once the provider traits have been discovered, other modules
//! in this crate can then process them in various ways
use crate::probe_spec::ProbeSpecification;
use heck::SnakeCase;
use syn::spanned::Spanned;
use syn::{ItemTrait, TraitItem};

use crate::{ProberError, ProberResult};

/// Looking at the methods defined on the trait, deduce from those methods the probes that we will
/// need to define, including their arg counts and arg types.
///
/// If the trait contains anything other than method declarations, or any of the declarations are
/// not suitable as probes, an error is returned
pub(crate) fn get_probes(item: &ItemTrait) -> ProberResult<Vec<ProbeSpecification>> {
    if item.generics.type_params().next() != None || item.generics.lifetimes().next() != None {
        return Err(ProberError::new(
            "Probe traits must not take any lifetime or type parameters",
            item.span(),
        ));
    }

    // Look at the methods on the trait and translate each one into a probe specification
    let mut specs: Vec<ProbeSpecification> = Vec::new();
    for f in item.items.iter() {
        match f {
            TraitItem::Method(ref m) => {
                specs.push(ProbeSpecification::from_method(item, m)?);
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

pub(crate) fn get_provider_name(item: &ItemTrait) -> String {
    // The provider name must be chosen carefully.  As of this writing (2019-04) the `bpftrace`
    // and `bcc` tools have, shall we say, "evolving" support for USDT.  As of now, with the
    // latest git version of `bpftrace`, the provider name can't have dots or colons.  For now,
    // then, the provider name is just the name of the provider trait, converted into
    // snake_case for consistency with USDT naming conventions.  If two modules in the same
    // process have the same provider name, they will conflict and some unspecified `bad
    // things` will happen.
    item.ident.to_string().to_snake_case()
}

#[cfg(test)]
mod test {
    use super::*;
    use quote::quote;
    use syn::{parse_quote, ItemTrait};

    mod data {
        use super::*;

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
        assert_eq!(true, get_probes(&data::simple_valid()).is_ok());
        assert_eq!(true, get_probes(&data::valid_with_many_refs()).is_ok());
    }

    #[test]
    fn trait_type_params_not_allowed() {
        // We need to be able to programmatically generate an impl of the probe trait which means
        // it cannot take any type parameters which we would not know how to provide
        assert_eq!(true, get_probes(&data::has_trait_type_param()).is_err());
    }

    #[test]
    fn non_method_items_not_allowed() {
        // A probe trait can't have anything other than methods.  That means no types, consts, etc
        assert_eq!(true, get_probes(&data::has_const()).is_err());
        assert_eq!(true, get_probes(&data::has_type()).is_err());
        assert_eq!(true, get_probes(&data::has_macro_invocation()).is_err());
    }

    #[test]
    fn method_modifiers_not_allowed() {
        // None of the Rust method modifiers like const, unsafe, async, or extern are allowed
        assert_eq!(true, get_probes(&data::has_const_fn()).is_err());
        assert_eq!(true, get_probes(&data::has_unsafe_fn()).is_err());
        assert_eq!(true, get_probes(&data::has_extern_fn()).is_err());
    }

    #[test]
    fn generic_methods_not_allowed() {
        // Probe methods must not be generic
        assert_eq!(true, get_probes(&data::has_fn_type_param()).is_err());
    }

    #[test]
    fn method_retvals_not_allowed() {
        // Probe methods never return a value.  I would like to be able to support methods that
        // explicitly return `()`, but it wasn't immediately obvious how to do that with `syn` and
        // it's more convenient to declare probe methods without any return type anyway
        assert_eq!(true, get_probes(&data::has_explicit_unit_retval()).is_err());
        assert_eq!(true, get_probes(&data::has_non_unit_retval()).is_err());
    }

    #[test]
    fn methods_must_not_take_self() {
        // Probe methods should not take a `self` parameter
        assert_eq!(true, get_probes(&data::has_non_static_method()).is_err());
        assert_eq!(true, get_probes(&data::has_mut_self_method()).is_err());
        assert_eq!(true, get_probes(&data::has_self_by_val_method()).is_err());
    }

    #[test]
    fn methods_must_not_have_default_impl() {
        // The whole point of this macro is to generate implementations of the probe methods so ti
        // doesn't make sense for the caller to provide their own
        assert_eq!(true, get_probes(&data::has_default_impl()).is_err());
    }
}
