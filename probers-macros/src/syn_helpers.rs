//! This module contains some helpers for working with the `syn` crate.  They grew to be
//! sufficiently complex as to merit a standalone module with separate tests.
use super::*;

/// In the `probers` macro implementation there are some cases where, given a `Type` instance, I
/// want to recursively visit all nested types.  For example, consider this type expression:
///
/// ```noexecute
/// Option<Result<Box<&str>>>
/// ```
///
/// `syn` will parse this into a single `syn::Type` struct, covering the whole type expression.
/// But if we dig into it, there will be an `Option`, a `Result`, a `Box`, and a `&str`.  This is
/// actually a pretty simple example, it gets much more complex than that.
///
/// If you want to either traverse all of those types building up some kind of list, or if you want
/// to permute those types, it's not so straightforward.  This method does that, and passes a
/// ref to every instance of `syn::Type` to your closure, expecting your closure to apply some kind
/// of transformation.  Originally I designed this so the closure would take a `&mut syn::Type`
/// however I could not figure out how to make that work with a `FnMut` that is called recursively
/// to the borrow checker's satisfaction.
pub(super) fn transform_types<F: FnMut(&syn::Type) -> ProberResult<syn::Type>>(
    typ: &syn::Type,
    mut f: F,
) -> ProberResult<syn::Type> {
    recurse_tree(&mut |t| f(t), typ)
}

fn recurse_tree<F: FnMut(&syn::Type) -> ProberResult<syn::Type>>(
    f: &mut F,
    typ: &syn::Type,
) -> ProberResult<syn::Type> {
    let mut new_typ = f(typ)?;

    //If this type itself takes type parameters, explore those recursively looking for
    //other reference types, all of which will require lifetime annotations.
    match new_typ {
        syn::Type::Slice(ref mut s) => {
            s.elem = Box::new(recurse_tree(f, &s.elem)?);
        }
        syn::Type::Array(ref mut a) => {
            a.elem = Box::new(recurse_tree(f, &a.elem)?);
        }
        syn::Type::Ptr(ref mut p) => {
            p.elem = Box::new(recurse_tree(f, &p.elem)?);
        }
        syn::Type::Reference(ref mut r) => {
            r.elem = Box::new(recurse_tree(f, &r.elem)?);
        }
        syn::Type::BareFn(ref mut func) => {
            //Each of the types in this bare function need to be examined
            if let syn::ReturnType::Type(_, ref mut typ) = func.output {
                *typ = Box::new(recurse_tree(f, &typ)?)
            }

            for arg in func.inputs.iter_mut() {
                arg.ty = recurse_tree(f, &arg.ty)?;
            }
        }
        syn::Type::Tuple(ref mut t) => {
            for ty in t.elems.iter_mut() {
                *ty = recurse_tree(f, ty)?;
            }
        }
        syn::Type::Path(ref mut p) => {
            p.path = recurse_path(f, &p.path)?;
        }
        syn::Type::TraitObject(ref mut t) => {
            for bound in t.bounds.iter_mut() {
                if let syn::TypeParamBound::Trait(ref mut tr) = bound {
                    tr.path = recurse_path(f, &tr.path)?;
                }
            }
        }
        syn::Type::ImplTrait(ref mut t) => {
            for bound in t.bounds.iter_mut() {
                if let syn::TypeParamBound::Trait(ref mut tr) = bound {
                    tr.path = recurse_path(f, &tr.path)?;
                }
            }
        }
        syn::Type::Paren(ref mut p) => {
            p.elem = Box::new(recurse_tree(f, &p.elem)?);
        }
        syn::Type::Group(ref mut g) => {
            g.elem = Box::new(recurse_tree(f, &g.elem)?);
        }
        syn::Type::Infer(_)
        | syn::Type::Never(_)
        | syn::Type::Macro(_)
        | syn::Type::Verbatim(_) => {
            //Nothing to do here there's no type information to recurse
        }
    };

    Ok(new_typ)
}

/// `Path` is something of a special case that is complex enough to require its own
/// function and which appears in multiple places.
fn recurse_path<F: FnMut(&syn::Type) -> ProberResult<syn::Type>>(
    f: &mut F,
    path: &syn::Path,
) -> ProberResult<syn::Path> {
    let mut path = path.clone();
    for seg in path.segments.iter_mut() {
        match seg.arguments {
            syn::PathArguments::None => {}
            syn::PathArguments::AngleBracketed(ref mut args) => {
                for arg in args.args.iter_mut() {
                    if let syn::GenericArgument::Type(ref mut typ) = arg {
                        *typ = recurse_tree(f, typ)?;
                    }
                }
            }
            syn::PathArguments::Parenthesized(ref mut args) => {
                if let syn::ReturnType::Type(_, ref mut typ) = args.output {
                    *typ = Box::new(recurse_tree(f, &typ)?)
                }

                for arg in args.inputs.iter_mut() {
                    *arg = recurse_tree(f, arg)?;
                }
            }
        }
    }

    Ok(path)
}

/// Helper method which takes as input an `Ident` which represents a variable or type name, appends
/// a given suffix to that name, and returns it as a new `Ident`
pub(crate) fn add_suffix_to_ident(ident: &Ident, suffix: &str) -> Ident {
    Ident::new(&format!("{}{}", ident, suffix), ident.span())
}

#[cfg(test)]
mod test {
    use super::*;
    use syn::parse_quote;

    /// Gets test data, where each test case is a tuple with the input type, and the output type
    /// with all references given a lifetime of `'mylife'`
    fn get_test_data() -> Vec<(syn::Type, syn::Type)> {
        vec![
            (parse_quote! { usize }, parse_quote! { usize }),
            (parse_quote! { &str }, parse_quote! { &'mylife str }),
            (
                parse_quote! { &'lifetime str },
                parse_quote! { &'mylife str },
            ),
            (parse_quote! { Option<u32> }, parse_quote! { Option<u32> }),
            (
                parse_quote! { &Option<u32> },
                parse_quote! { &'mylife Option<u32> },
            ),
            (
                parse_quote! { Option<Result<&str>, Option<String>> },
                parse_quote! { Option<Result<&'mylife str>, Option<String>> },
            ),
            (
                parse_quote! { Option<Option<&Option<Option<&str>>>> },
                parse_quote! { Option<Option<&'mylife Option<Option<&'mylife str>>>> },
            ),
        ]
    }

    #[test]
    fn identity_function() {
        // When using a closure that just returns the input, the transformation should not alter
        // the resulting type
        for typ in get_test_data().iter().map(|(input, _)| input) {
            let new_typ = transform_types(typ, |t| Ok(t.clone())).unwrap();

            let expected_tokens = quote! { #typ };
            let actual_tokens = quote! { #new_typ };

            assert_eq!(expected_tokens.to_string(), actual_tokens.to_string());
        }
    }

    #[test]
    fn add_lifetimes() {
        //As a more realistic test of what this does, try adding lifetimes to every reference arg
        for (typ, expectedtyp) in get_test_data().iter() {
            let new_typ = transform_types(typ, |t| {
                let mut new_typ = t.clone();
                if let syn::Type::Reference(ref mut tr) = new_typ {
                    tr.lifetime = Some(syn::Lifetime::new(
                        "'mylife",
                        proc_macro2::Span::call_site(),
                    ));
                }

                Ok(new_typ)
            })
            .unwrap();

            let expected_tokens = quote! { #expectedtyp };
            let actual_tokens = quote! { #new_typ };

            assert_eq!(expected_tokens.to_string(), actual_tokens.to_string());
        }
    }
}
