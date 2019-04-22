//! Defines an error type which can represent all the various kinds of failures the code gen
//! process can encounter.  More specifically to this use case, the error type can also produce a
//! `TokenStream` which, when consumed by the `rustc` compiler, causes the compile to fail with a
//! compiler error with the text of whatever the error message was.  That's how the proc macros
//! communicate failure details back to the compiler at compile time.

use failure::Fail;
use proc_macro2::Span;
use quote::ToTokens;
use std::fmt::Display;
use std::sync::{Arc, Mutex};
//use syn::spanned::Spanned;

type SynError = Arc<Mutex<syn::Error>>;

fn new_syn_error<T: ToTokens, U: Display>(message: U, tokens: T) -> SynError {
    wrap_syn_error(syn::Error::new_spanned(tokens, message))
}

fn new_syn_error_span<U: Display>(message: U, element: Span) -> SynError {
    wrap_syn_error(syn::Error::new(element, message))
}

fn wrap_syn_error(e: syn::Error) -> SynError {
    Arc::new(Mutex::new(e))
}

#[derive(Debug, Fail)]
pub enum ProbersError {
    #[fail(display = "There's a problem with this provider trait: {}", details)]
    InvalidProvider {
        details: String,
        syn_error: SynError,
    },

    #[fail(display = "Legacy ProberError: {}", message)]
    LegacyProberError {
        message: String,
        syn_error: SynError,
    },

    #[fail(display = "Parse error: {}", message)]
    SynError {
        message: String,

        syn_error: SynError,
    },

    #[fail(display = "Invalid call expression: {}", message)]
    InvalidCallExpression {
        message: String,
        syn_error: SynError,
    },
}

unsafe impl Send for ProbersError {}
unsafe impl Sync for ProbersError {}

impl ProbersError {
    pub fn invalid_provider<T: ToTokens>(message: &'static str, element: T) -> ProbersError {
        ProbersError::InvalidProvider {
            details: message.to_owned(),
            syn_error: new_syn_error(message, element),
        }
    }

    pub fn legacy_prober_error(e: crate::ProberError) -> ProbersError {
        ProbersError::LegacyProberError {
            message: e.message.clone(),
            syn_error: new_syn_error_span(e.message, e.span),
        }
    }

    pub fn syn_error(message: impl AsRef<str>, e: syn::Error) -> ProbersError {
        ProbersError::SynError {
            message: message.as_ref().to_owned(),
            syn_error: wrap_syn_error(e),
        }
    }

    pub fn invalid_call_expression<T: ToTokens>(message: String, element: T) -> ProbersError {
        ProbersError::InvalidCallExpression {
            message: message.clone(),
            syn_error: new_syn_error(message, element),
        }
    }
}

pub type ProbersResult<T> = std::result::Result<T, ProbersError>;
