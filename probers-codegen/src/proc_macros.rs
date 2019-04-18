//! This module is where the *implementation* of the probe-related proc macros are.  The actual
//! proc macro is in the `probers-macros` crate because proc macro crates can _only_ export proc
//! macros and nothing else.  That's an inconvenient restriction, especially since there's quite a
//! lot of overlap between the macro code and the build-time probe code generation logic.  Hence,
//! this bifurcation.
use crate::probe;
use crate::probe_call::ProbeCall;
use crate::provider;
use crate::provider::ProviderSpecification;
use crate::{ProberError, ProberResult};
use heck::{ShoutySnakeCase, SnakeCase};
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use std::borrow::BorrowMut;
use std::fmt::Display;
use syn::parse_quote;
use syn::spanned::Spanned;
use syn::{Ident, ItemTrait};

use crate::{CodeGenerator, Generator};

/// Uses the `syn` library's `Error` struct to report an error in the form of a `TokenStream`, so
/// that a proc macro can insert this token stream into its output and thereby report a detailed
/// error message to the user.
///
/// The span of this error corresponds to the `tokens` parameter, so the user gets the relevant
/// context for the error
pub fn report_error<T: quote::ToTokens, U: Display>(tokens: &T, message: U) -> TokenStream {
    syn::Error::new_spanned(tokens.clone(), message).to_compile_error()
}

/// Translates what looks to be an explicit call to the associated function corresponding to a
/// probe on a provider trait, into something which at runtime will most efficiently attempt to
/// access the global static instance of the probe and, if it's enabled, evaluate the args and fire
/// the probe.
///
/// It translates something like this:
///
/// ```noexecute
/// probe!(MyProvider::myprobe(1, 5, "this is a string", compute_something()));
/// ```
///
/// into:
///
/// ```noexecute
/// {
///     if let Some(probe) = MyProvider::get_myprobe_probe() {
///         if probe.is_enabled() {
///             probe.fire((1, 5, "this is a string", compute_something(),)));
///         }
///     }
/// }
/// ```
///
/// In particular, note that the probe's parameters are not evaluated unless the provider
/// initialized successfully and the probe is enabled.
pub fn probe_impl(tokens: TokenStream) -> ProberResult<TokenStream> {
    Generator::handle_probe_call(ProbeCall::from_token_stream(tokens)?)
}

pub fn init_provider_impl(typ: syn::TypePath) -> ProberResult<TokenStream> {
    let span = typ.span();
    Ok(quote_spanned! {span=>
        #typ::__try_init_provider()
    })
}

/// Actual implementation of the macro logic, factored out of the proc macro itself so that it's
/// more testable
pub fn prober_impl(item: ItemTrait) -> ProberResult<TokenStream> {
    Generator::handle_provider_trait(&item)
}

#[cfg(test)]
mod test {}
