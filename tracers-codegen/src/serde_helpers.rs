//! This module contains some internal helpers to provide field-expedient ser/deser of some types
//! that do not and cannot support the `serde` serialize/deserialize traits, but can be represented
//! as a string
use failure::format_err;
use proc_macro2::TokenStream;
use serde::Deserializer;
use serde::Serializer;

/// Serialize and deserialize most `syn` crate types as strings, reparsing them on
/// deserialization
pub(crate) mod syn {
    use super::*;
    use serde::de::*;

    pub(crate) fn serialize<S: Serializer, T: ::syn::parse::Parse + quote::ToTokens>(
        x: &T,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let mut tokens = TokenStream::new();
        x.to_tokens(&mut tokens);

        s.serialize_str(&tokens.to_string())
    }

    pub(crate) fn deserialize<
        'de,
        D: Deserializer<'de>,
        T: ::syn::parse::Parse + quote::ToTokens,
    >(
        d: D,
    ) -> Result<T, D::Error> {
        let as_str = String::deserialize::<D>(d)?;

        let deserialized: T = ::syn::parse_str(&as_str).map_err(D::Error::custom)?;

        Ok(deserialized)
    }
}

/// Implements serialization and deserialization of token streams, using their string
/// representation
pub(crate) mod token_stream {
    use super::*;
    use serde::de::*;
    use std::str::FromStr;

    pub(crate) fn serialize<S: Serializer>(x: &TokenStream, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&x.to_string())
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<TokenStream, D::Error> {
        let as_str = String::deserialize::<D>(d)?;

        let tokens = TokenStream::from_str(&as_str)
            .map_err(|e| D::Error::custom(format!("proc_macro2 LexError: {:?}", e)))?;

        Ok(tokens)
    }
}

/// Serializes the `::syn::PatIdent` struct
pub(crate) mod pat_ident {
    use super::*;
    use serde::de::*;

    pub(crate) fn serialize<S: Serializer>(x: &::syn::PatIdent, s: S) -> Result<S::Ok, S::Error> {
        //Turn into a `Pat` enum, which can be serialized with the general `syn` serializer
        let pat: ::syn::Pat = x.clone().into();

        super::syn::serialize(&pat, s)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<::syn::PatIdent, D::Error> {
        let pat = super::syn::deserialize(d)?;

        match pat {
            ::syn::Pat::Ident(pat_ident) => Ok(pat_ident),
            _ => Err(D::Error::custom(format_err!(
                "WTF got back a non-ident pattern"
            ))),
        }
    }
}

/// Provides a placeholder ser/de implementation for `proc_macro2::Span`.  In fact this can't be
/// serialized because it references a specific part of a file, so this serialization just skips it
/// entirely.  We don't ever use the span from the deserialized struct anyway, it's only used by
/// proc macros.
pub(crate) mod span {
    use super::*;
    use proc_macro2::Span;
    use serde::de::*;

    pub(crate) fn serialize<S: Serializer>(_x: &Span, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_unit()
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Span, D::Error> {
        let _ = <()>::deserialize::<D>(d)?;
        Ok(Span::call_site())
    }
}

pub(crate) extern crate serde_str as string;
