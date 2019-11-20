//! This module.res and implements the `ProbeSpecification` struct, which represents a single
//! tracing probe corresponding to a single method on the trait which is marked with a `#[tracer]`
//! attribute.
//!
//! This module encapsulates most of the messy details of translating a simple trait method into
//! the definition of a probe.

use crate::serde_helpers;
use crate::spec::ProbeArgSpecification;
use crate::{TracersError, TracersResult};
use proc_macro2::Span;
use serde::{Deserialize, Serialize};
use std::fmt;
use syn::spanned::Spanned;
use syn::Visibility;
use syn::{FnArg, Ident, ItemTrait, ReturnType, TraitItemMethod};

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct ProbeSpecification {
    pub name: String,
    #[serde(with = "serde_helpers::syn")]
    pub method_name: Ident,
    #[serde(with = "serde_helpers::syn")]
    pub original_method: TraitItemMethod,
    #[serde(with = "serde_helpers::syn")]
    pub vis: Visibility,
    #[serde(with = "serde_helpers::span")]
    pub span: Span,
    pub args: Vec<ProbeArgSpecification>,
}

impl fmt::Debug for ProbeSpecification {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ProbeSpecification(name={}, args=(", self.name)?;

        for arg in self.args.iter() {
            writeln!(f, "{:?}", arg)?;
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
    ) -> TracersResult<ProbeSpecification> {
        if method.default != None {
            return Err(TracersError::invalid_provider(
                "Probe methods must NOT have a default implementation",
                method,
            ));
        } else if method.sig.constness != None
            || method.sig.unsafety != None
            || method.sig.asyncness != None
            || method.sig.abi != None
        {
            return Err(TracersError::invalid_provider(
                "Probe methods cannot be `const`, `unsafe`, `async`, or `extern \"C\"`",
                method,
            ));
        } else if method.sig.generics.type_params().next() != None {
            return Err(TracersError::invalid_provider(
            "Probe methods must not take any type parameters; generics are not supported in probes",
            method,
        ));
        } else if method.sig.variadic != None {
            return Err(TracersError::invalid_provider(
                "Probe methods cannot have variadic arguments",
                method,
            ));
        } else if method.sig.output != ReturnType::Default {
            return Err(TracersError::invalid_provider(
                "Probe methods must not have an explicit return type (they return `()` implicitly)",
                method,
            ));
        };

        let first_arg = method.sig.inputs.iter().next();
        if let Some(FnArg::Receiver(_)) = first_arg {
            return Err(TracersError::invalid_provider(
                "Probe methods must not have any `&self` or `self` args",
                method,
            ));
        }

        let mut args: Vec<ProbeArgSpecification> = Vec::new();
        for (idx, arg) in method.sig.inputs.iter().enumerate() {
            args.push(ProbeArgSpecification::from_fnarg(method, idx, arg)?);
        }

        let spec = ProbeSpecification {
            name: method.sig.ident.to_string(),
            method_name: method.sig.ident.clone(),
            original_method: method.clone(),
            vis: item.vis.clone(),
            span: method.span(),
            args,
        };

        Ok(spec)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::syn_helpers;
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

                        self_arg_name.ident == *spec_arg_name && self_arg_type == spec_arg_type
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

            ProbeSpecification::from_method(&data::trait_item(), input).unwrap_or_else(|_| {
                panic!(format!(
                    "This should be treated as a valid method: {}",
                    input_string
                ))
            });
        }
    }

    #[test]
    fn works_with_invalid_cases() {
        for input in data::invalid_test_cases().iter() {
            let input_string = quote! { #input }.to_string();

            ProbeSpecification::from_method(&data::trait_item(), input)
                .err()
                .unwrap_or_else(|| {
                    panic!(format!(
                        "This should be treated as an invalid method: {}",
                        input_string
                    ))
                });
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
            let probe = ProbeSpecification::from_method(&data::trait_item(), method)
                .unwrap_or_else(|_| {
                    panic!(format!(
                        "This method should be valid: {}",
                        syn_helpers::convert_to_string(method)
                    ))
                });

            //Re-construct the probe method using the args as they've been computed in the
            //ProbeSpecification.  The lifetimes should be present
            let args = probe.args.iter().map(|arg| {
                let (nam, typ) = (&arg.ident(), &arg.syn_typ_with_lifetimes());
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
