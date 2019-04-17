//! This module is concerned with parsing and interpreting the arguments to a probe

use crate::argtypes::{from_syn_type, ArgTypeInfo};
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use std::fmt;
use syn::spanned::Spanned;
use syn::Visibility;
use syn::{FnArg, Ident, ItemTrait, ReturnType, TraitItemMethod};

use crate::argtypes;
use crate::syn_helpers;
use crate::{ProberError, ProberResult};

pub(crate) struct ProbeArgSpecification {
    pub name: String,
    pub ident: syn::PatIdent,
    pub syn_typ: syn::Type,
    pub art_type_info: ArgTypeInfo,
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
    pub fn from_fnarg(arg: &syn::FnArg) -> ProberResult<ProbeArgSpecification> {
        //Apologies for the crazy match expression.  Rust's AST is a complicated beast
        //Many things can be function arguments in Rust; we only support the very basic form of:
        //`arg_name: some_type`
        if let FnArg::Captured(syn::ArgCaptured {
            pat: syn::Pat::Ident(pat_ident),
            ty,
            ..
        }) = arg
        {
            Self::from_ident_type_pair(pat_ident, ty)
        } else {
            Err(ProberError::new(
            &format!("Probe method arguments should be in the form `name: TypeName`; {:?} is not an expected argument", arg),
            arg.span(),
        ))
        }
    }

    pub fn from_ident_type_pair(
        ident: &syn::PatIdent,
        typ: &syn::Type,
    ) -> ProberResult<ProbeArgSpecification> {
        if let Some(art_type_info) = argtypes::from_syn_type(typ) {
            Ok(ProbeArgSpecification {
                name: ident.ident.to_string(),
                ident: ident.clone(),
                syn_typ: typ.clone(),
                art_type_info: art_type_info,
            })
        } else {
            Err(ProberError::new(
            &format!("The argument type '{:?}' of argument '{}' is not supported for probing.  Generally only the standard string, integer, and bool types, as well as references and Option's of the same, are supported",
                     typ,
                     ident.ident),
            typ.span(),
        ))
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    //Implement equality tests only for testing; in real use they're not needed
    impl PartialEq<ProbeArgSpecification> for ProbeArgSpecification {
        fn eq(&self, other: &ProbeArgSpecification) -> bool {
            self.name == other.name && self.ident == other.ident && self.syn_typ == other.syn_typ
        }
    }
}
