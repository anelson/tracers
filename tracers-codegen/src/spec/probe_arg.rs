//! This module is concerned with parsing and interpreting the arguments to a probe

use crate::argtypes;
use crate::argtypes::ArgTypeInfo;
use crate::serde_helpers;
use crate::syn_helpers;
use crate::{TracersError, TracersResult};
use serde::{Deserialize, Serialize};
use std::fmt;
use syn::spanned::Spanned;

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct ProbeArgSpecification {
    name: String,

    #[allow(dead_code)] //TODO: Temporary
    probe_name: String,

    #[allow(dead_code)] //TODO: Temporary
    ordinal: usize,

    #[serde(with = "serde_helpers::pat_ident")]
    ident: syn::PatIdent,

    #[serde(with = "serde_helpers::syn")]
    syn_typ: syn::Type,

    #[serde(with = "serde_helpers::syn")]
    syn_typ_with_lifetimes: syn::Type,

    arg_type_info: ArgTypeInfo,
}

impl fmt::Debug for ProbeArgSpecification {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "ProbeArgSpecification(name={}, syn_type={:?})",
            self.name, self.syn_typ
        )?;
        write!(f, ")")
    }
}

impl ProbeArgSpecification {
    pub fn from_fnarg(
        probe_method: &syn::TraitItemMethod,
        ordinal: usize,
        arg: &syn::FnArg,
    ) -> TracersResult<ProbeArgSpecification> {
        //Apologies for the crazy match expression.  Rust's AST is a complicated beast
        //Many things can be function arguments in Rust; we only support the very basic form of:
        //`arg_name: some_type`
        if let syn::FnArg::Typed(syn::PatType { pat, ty, .. }) = arg {
            if let syn::Pat::Ident(pat_ident) = pat.as_ref() {
                return Self::from_ident_type_pair(probe_method, ordinal, pat_ident, ty);
            }
        }
        return Err(TracersError::invalid_provider(
            format!("Probe method arguments should be in the form `name: TypeName`; {} is not an expected argument", syn_helpers::convert_to_string(arg)),
            arg,
            ));
    }

    /// Constructs a `ProbeArgSpecification` from information from a decomposed fn arg once it's
    /// been validated that the arg is in the expected `name: type` format.
    pub fn from_ident_type_pair(
        probe_method: &syn::TraitItemMethod,
        ordinal: usize,
        ident: &syn::PatIdent,
        typ: &syn::Type,
    ) -> TracersResult<ProbeArgSpecification> {
        //Note the type is annotated right here with the added lifetime information.  It's easier
        //and faster then to compute the annotations on the fly
        if let Some(arg_type_info) = argtypes::from_syn_type(typ) {
            let name = ident.ident.to_string();
            let probe_name = probe_method.sig.ident.to_string();
            let syn_typ = typ.clone();
            let syn_typ_with_lifetimes = Self::add_lifetimes_to_syn_type(&probe_name, &name, typ)?;
            Ok(ProbeArgSpecification {
                name,
                probe_name,
                ordinal,
                ident: ident.clone(),
                syn_typ,
                syn_typ_with_lifetimes,
                arg_type_info,
            })
        } else {
            return Err(TracersError::invalid_provider(
                    format!("The argument type '{}' of argument '{}' on probe '{}' is not supported for probing.  Generally only the standard string, integer, and bool types, as well as references and Option's of the same, are supported", syn_helpers::convert_to_string(typ), ident.ident, probe_method.sig.ident), typ,
            ));
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn ident(&self) -> &syn::PatIdent {
        &self.ident
    }

    pub fn arg_type_info(&self) -> &ArgTypeInfo {
        &self.arg_type_info
    }

    pub fn syn_typ(&self) -> &syn::Type {
        &self.syn_typ
    }

    /// Returns the Rust AST representation of this argument's type, with lifetime annotations
    /// added for every reference type.  See `add_lifetimes_to_syn_type` for more details;
    pub fn syn_typ_with_lifetimes(&self) -> &syn::Type {
        &self.syn_typ_with_lifetimes
    }

    /// Scans the argument's type information after it has been annotated with explicit lifetimes
    /// for every reference, and returns a vector of a copy of those lifetimes by themselves,
    /// separate from any time information
    pub fn lifetimes(&self) -> Vec<syn::Lifetime> {
        let mut lifetimes = Vec::new();

        // Traverse the types tree, pulling out the lifetimes and putting them into a list.
        let _ = syn_helpers::transform_types(&self.syn_typ_with_lifetimes, |typ| {
            let typ = typ.clone();
            if let syn::Type::Reference(ref tr) = typ {
                if let Some(ref lt) = tr.lifetime {
                    lifetimes.push(lt.clone());
                }
            }

            Ok(typ)
        })
        .unwrap(); //unwrap is safe here because our closure above has no failure path

        lifetimes
    }

    /// Re-writes an argument's type information, annotating all references with unique lifetimes
    ///
    /// Probe function arguments very often will include reference types, probably typically things
    /// like `&str` or `&String` but more exotic variations are possible.  When declaring these
    /// functions Rust lifetime elision means the programmer (almost) never has to specify the
    /// lifetime associated with a reference type, but when implementing the probing code we have
    /// to express these arguments in the form of a tuple type not attached to any specific
    /// function; that is to say, we don't get the luxury of eliding lifetimes and must be explicit
    /// about the lifetimes of our references.
    ///
    /// Thus, this method.  It scans the arguments for reference types, and it does so recursively.
    /// That is to say, it catches `&str`, but is also catches `Option<Result<&Option<&str>,
    /// Error>>`.  In each case, every time you see a `&` in a type, a lifetime must be created.
    /// This method returns a modified copy of the `syn` library's parse tree types, with the
    /// lifetimes added next to each reference.
    ///
    /// Each lifetime name is unique, and is derived from both the name of the probe method and the
    /// name of the argument.  So within a provider, every lifetime parameter will have a unique
    /// name.  That's important because the `struct` we declare as part of the implementation of
    /// the tracer trait maintains each of the probe's strongly-typed wrappers, and thus it will
    /// take _all_ of the lifetime parameters for _all_ of the probe functions.
    ///
    /// This works by scaning each of the probe arguments to build up a list of all of the lifetime parameters, by
    /// argument.  This is more complicated than it might first appear because reference types can
    /// be nested quite deeply in the type expression.  Here are some examples of some probe
    /// defintions and the implicit lifetimes which in our code we must make explicit:
    ///
    /// ```noexecute
    /// trait Foo {
    ///	    fn probe0(); // None
    ///	    fn probe1(arg: usize); // None
    ///	    fn probe2(arg: &str); // 'probe2_arg_1
    ///	    fn probe3(arg: Option<&str>); // 'probe3_arg_1

    ///	    //'probe4_arg_1, 'probe4_arg_2, 'probe4_arg_3
    ///	    fn probe4(arg: &Option<Result<&String, &u32>>);
    /// }
    /// ```
    fn add_lifetimes_to_syn_type(
        probe_name: &str,
        arg_name: &str,
        syn_typ: &syn::Type,
    ) -> TracersResult<syn::Type> {
        fn generate_lifetime(
            probe_name: &str,
            arg_name: &str,
            syn_typ: &syn::Type,
            count: &mut usize,
        ) -> syn::Lifetime {
            *count += 1;
            syn::Lifetime::new(
                &format!("'{}_{}_{}", probe_name, arg_name, count),
                syn_typ.span(),
            )
        }

        let mut count: usize = 0;
        syn_helpers::transform_types(syn_typ, |typ: &syn::Type| {
            let mut new_typ = typ.clone();

            if let syn::Type::Reference(ref mut tr) = new_typ {
                tr.lifetime = Some(generate_lifetime(probe_name, arg_name, &typ, &mut count));
            }

            Ok(new_typ)
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use quote::quote;
    use syn::parse_quote;

    //Implement equality tests only for testing; in real use they're not needed
    impl PartialEq<ProbeArgSpecification> for ProbeArgSpecification {
        fn eq(&self, other: &ProbeArgSpecification) -> bool {
            self.name == other.name && self.ident == other.ident && self.syn_typ == other.syn_typ
        }
    }

    struct TestCase {
        probe_method: syn::TraitItemMethod,
        expected_error: Option<&'static str>,
        arg_name: &'static str,
        arg_type: syn::Type,
        arg_type_with_lifetimes: syn::Type,
        lifetimes: Vec<syn::Lifetime>,
    }

    impl TestCase {
        fn new(
            probe_method: proc_macro2::TokenStream,
            expected_error: impl Into<Option<&'static str>>,
            arg_name: &'static str,
            arg_type: syn::Type,
            arg_type_with_lifetimes: syn::Type,
            lifetimes: Vec<syn::Lifetime>,
        ) -> TestCase {
            TestCase {
                probe_method: parse_quote! { #probe_method },
                expected_error: expected_error.into(),
                arg_name,
                arg_type,
                arg_type_with_lifetimes,
                lifetimes,
            }
        }
    }

    macro_rules! test_case {
        ($expected_error:expr, $probe_name:ident, $arg_name:ident, $arg_type:ty, $arg_type_with_lifetimes:ty, $($lifetime:lifetime),*) => {
            TestCase::new(
                quote!{ fn $probe_name($arg_name: $arg_type); },
                $expected_error,
                stringify!($arg_name),
                parse_quote! { $arg_type },
                parse_quote! { $arg_type_with_lifetimes },
                vec![$(
                    parse_quote! { $lifetime }
                    ),*]
                )
        };
        //This is an overload for test cases in which there are no lifetimes expected
        ($expected_error:expr, $probe_name:ident, $arg_name:ident, $arg_type:ty) => {
            test_case!($expected_error, $probe_name, $arg_name, $arg_type, $arg_type, )
        };
    }

    fn get_test_cases() -> Vec<TestCase> {
        vec![
            test_case!(None, probe0, arg0, u8),
            test_case!(None, probe0, arg0, bool),
            test_case!(None, probe0, arg0, &str, &'probe0_arg0_1 str, 'probe0_arg0_1),
            test_case!(None, probe0, arg0, &String, &'probe0_arg0_1 String, 'probe0_arg0_1),
            test_case!(None, probe0, arg0, &Option<usize>, &'probe0_arg0_1 Option<usize>, 'probe0_arg0_1),
            test_case!(None, probe0, arg0, &Option<&str>, &'probe0_arg0_1 Option<&'probe0_arg0_2 str>, 'probe0_arg0_1, 'probe0_arg0_2),
        ]
    }

    fn get_arg_from_test_case(case: &TestCase) -> TracersResult<ProbeArgSpecification> {
        let tokens = &case.probe_method;
        let method: syn::TraitItemMethod = parse_quote! { #tokens };
        let arg = method
            .sig
            .inputs
            .iter()
            .next()
            .expect("expecting exactly one arg");

        ProbeArgSpecification::from_fnarg(&method, 0, &arg)
    }

    #[test]
    fn parses_valid_test_cases() {
        for (index, case) in get_test_cases()
            .into_iter()
            .enumerate()
            .filter(|(_, c)| c.expected_error.is_none())
        {
            let arg = get_arg_from_test_case(&case).expect("unexpected error parsing arg");

            assert_eq!(case.arg_name, arg.name, "test# {}", index);
            assert_eq!(
                syn_helpers::convert_to_string(&case.arg_type),
                syn_helpers::convert_to_string(&arg.syn_typ()),
                "test# {}",
                index
            );

            assert_eq!(
                syn_helpers::convert_to_string(&case.arg_type_with_lifetimes),
                syn_helpers::convert_to_string(arg.syn_typ_with_lifetimes()),
                "test# {}",
                index
            );

            //Asserting equality on lifetimes makes for some messy error messages if there's a
            //mismatch.  So assert on equality of their string representations
            let expected: Vec<_> = case
                .lifetimes
                .iter()
                .map(syn_helpers::convert_to_string)
                .collect();
            let actual: Vec<_> = arg
                .lifetimes()
                .iter()
                .map(syn_helpers::convert_to_string)
                .collect();
            assert_eq!(expected, actual, "test# {}", index);
        }
    }
}
