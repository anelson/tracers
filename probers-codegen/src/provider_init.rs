//! This module parses the tokens passed to the `init_provider!` macro, validates them, and
//! represents the tokens in a form that generators can easily make use of
use crate::syn_helpers;
use crate::{ProberError, ProberResult};
use heck::{ShoutySnakeCase, SnakeCase};
use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use std::borrow::BorrowMut;
use std::fmt;
use std::fmt::Display;
use syn::parse2;
use syn::parse_quote;
use syn::spanned::Spanned;
use syn::{Ident, ItemTrait};

/// This is probably the simplest of the probers specs.  Callers use the `init_provider!` macro to
/// perform initialization of the provider at some explicit point.  This isn't required, and
/// providers will lazily init on the first probe call if not initialized explicitly.
///
/// This is only here to support providers like `stap_usdt` which actually do some initialization
/// at runtime, and before this intialization happens no probes exist so probes can't even be
/// enumerated.  Thus it's a best practice for apps to explicitly initialize their providers at
/// startup time.
///
/// The syntax is simple:
///
/// ```no_execute
/// init_provider!(MyProviderTrait)
/// ```
///
/// where `MyProviderTrait` is a (possibly fully-qualified) path to a trait which was previously
/// decorated with the `prober` attribute.  It's realy that simple.
#[derive(PartialEq)]
pub struct ProviderInitSpecification {
    pub provider: syn::Path,
}

impl fmt::Debug for ProviderInitSpecification {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ProviderInitSpecification(")?;
        write!(
            f,
            "provider={}",
            syn_helpers::convert_to_string(&self.provider)
        )?;
        write!(f, ")")
    }
}

impl ProviderInitSpecification {
    /// Parses a token stream directly from the compiler, decomposing it into the details of the
    /// provider
    pub fn from_token_stream(tokens: TokenStream) -> ProberResult<ProviderInitSpecification> {
        match syn::parse2::<syn::Path>(tokens) {
            Ok(path) => Self::from_path(path),
            Err(e) => Err(ProberError::new(e.to_string(), e.span())),
        }
    }

    pub fn from_path(path: syn::Path) -> ProberResult<ProviderInitSpecification> {
        Ok(ProviderInitSpecification { provider: path })
    }
}
