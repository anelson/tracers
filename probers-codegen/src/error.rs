//! Defines an error type which can represent all the various kinds of failures the code gen
//! process can encounter.  More specifically to this use case, the error type can also produce a
//! `TokenStream` which, when consumed by the `rustc` compiler, causes the compile to fail with a
//! compiler error with the text of whatever the error message was.  That's how the proc macros
//! communicate failure details back to the compiler at compile time.

use failure::Fail;
use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::ToTokens;
use std::fmt::Display;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

type SynError = Arc<Mutex<syn::Error>>;

#[derive(Debug, Fail)]
pub enum ProbersError {
    #[fail(display = "There's a problem with this provider trait: {}", details)]
    InvalidProvider {
        details: String,
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

    #[fail(display = "{}", message)]
    OtherError {
        message: String,
        #[fail(cause)]
        error: failure::Error,
    },

    #[fail(
        display = "Unable to read build info from '{}'.\nAre you sure you're calling `probers_build::build()` in your `build.rs`?\nError cause: {}",
        build_info_path, message
    )]
    BuildInfoReadError {
        message: String,
        build_info_path: String,
        #[fail(cause)]
        error: failure::Error,
    },

    #[fail(
        display = "Unable to write build info from '{}'.\nAre you sure you're calling `probers_build::build()` in your `build.rs`?\nError cause: {}",
        build_info_path, message
    )]
    BuildInfoWriteError {
        message: String,
        build_info_path: String,
        #[fail(cause)]
        error: failure::Error,
    },
}

unsafe impl Send for ProbersError {}
unsafe impl Sync for ProbersError {}

impl PartialEq<ProbersError> for ProbersError {
    fn eq(&self, other: &ProbersError) -> bool {
        //There are a lot of types that don't support equality.  To keep it easy, just compare the
        //display version
        self.to_string() == other.to_string()
    }
}

impl ProbersError {
    pub fn invalid_provider<T: ToTokens>(message: impl AsRef<str>, element: T) -> ProbersError {
        ProbersError::InvalidProvider {
            details: message.as_ref().to_owned(),
            syn_error: Self::new_syn_error(message.as_ref(), element),
        }
    }

    pub fn syn_error(message: impl AsRef<str>, e: syn::Error) -> ProbersError {
        ProbersError::SynError {
            message: format!("{}: {}", message.as_ref(), e),
            syn_error: Self::wrap_syn_error(e),
        }
    }

    pub fn invalid_call_expression<T: ToTokens>(
        message: impl AsRef<str>,
        element: T,
    ) -> ProbersError {
        ProbersError::InvalidCallExpression {
            message: message.as_ref().to_owned(),
            syn_error: Self::new_syn_error(message.as_ref(), element),
        }
    }

    pub fn other_error<E: Into<failure::Error>>(e: E) -> ProbersError {
        let e = e.into();

        ProbersError::OtherError {
            message: e.to_string(),
            error: e,
        }
    }

    pub fn build_info_read_error(build_info_path: PathBuf, e: failure::Error) -> ProbersError {
        ProbersError::BuildInfoReadError {
            message: e.to_string(),
            build_info_path: build_info_path.display().to_string(),
            error: e,
        }
    }

    pub fn build_info_write_error(build_info_path: PathBuf, e: failure::Error) -> ProbersError {
        ProbersError::BuildInfoWriteError {
            message: e.to_string(),
            build_info_path: build_info_path.display().to_string(),
            error: e,
        }
    }

    /// Converts this error type into a `syn::Error`, preserving context from spans and elements if
    /// any were given
    pub fn into_syn_error(self) -> syn::Error {
        match self {
            ProbersError::InvalidProvider { syn_error, .. } => Self::unwrap_syn_error(syn_error),
            ProbersError::SynError { syn_error, .. } => Self::unwrap_syn_error(syn_error),
            ProbersError::InvalidCallExpression { syn_error, .. } => {
                Self::unwrap_syn_error(syn_error)
            }
            ProbersError::OtherError { message, error } => {
                syn::Error::new(Span::call_site(), format!("{}: {}", message, error))
            }
            others => syn::Error::new(Span::call_site(), others.to_string()),
        }
    }

    /// Convert this error into a `TokenStream` such that when the compiler consumes the token
    /// stream it will evaluate to a compile error, with the span corresponding to whatever element
    /// was used to report the error.  For those error types that don't have a corresponding
    /// element, the call site of the macro will be used
    pub fn into_compiler_error(self) -> TokenStream {
        self.into_syn_error().to_compile_error()
    }

    fn new_syn_error<T: ToTokens, U: Display>(message: U, tokens: T) -> SynError {
        Self::wrap_syn_error(syn::Error::new_spanned(tokens, message))
    }

    fn wrap_syn_error(e: syn::Error) -> SynError {
        Arc::new(Mutex::new(e))
    }

    fn unwrap_syn_error(e: SynError) -> syn::Error {
        e.lock().unwrap().clone()
    }
}

impl From<failure::Error> for ProbersError {
    fn from(failure: failure::Error) -> ProbersError {
        ProbersError::other_error(failure)
    }
}

pub type ProbersResult<T> = std::result::Result<T, ProbersError>;
