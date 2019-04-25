//! Defines an error type which can represent all the various kinds of failures the code gen
//! process can encounter.  More specifically to this use case, the error type can also produce a
//! `TokenStream` which, when consumed by the `rustc` compiler, causes the compile to fail with a
//! compiler error with the text of whatever the error message was.  That's how the proc macros
//! communicate failure details back to the compiler at compile time.

use failure::{Error, Fail};
use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::ToTokens;
use std::fmt;
use std::fmt::Display;
use std::path::PathBuf;

#[derive(Debug, Fail)]
pub enum TracersError {
    InvalidProvider {
        message: String,
        #[fail(cause)]
        syn_error: Error,
    },

    SynError {
        message: String,
        #[fail(cause)]
        syn_error: Error,
    },

    InvalidCallExpression {
        message: String,
        #[fail(cause)]
        syn_error: Error,
    },

    OtherError {
        message: String,
        #[fail(cause)]
        error: Error,
    },

    MissingCallInBuildRs,

    BuildInfoReadError {
        message: String,
        build_info_path: String,
        #[fail(cause)]
        error: Error,
    },

    BuildInfoWriteError {
        message: String,
        build_info_path: String,
        #[fail(cause)]
        error: Error,
    },

    CodeGenerationError {
        message: String,
    },
}

impl Display for TracersError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TracersError::InvalidProvider { message, .. } => write!(f, "{}", message),
            TracersError::SynError { message, .. } => write!(f, "{}", message),
            TracersError::InvalidCallExpression { message, .. } => write!(f, "{}", message),
            TracersError::OtherError { message, .. } => write!(f, "{}", message),
            TracersError::MissingCallInBuildRs => write!(f, "Build environment is incomplete; make sure you are calling `tracers_build::build()` in your `build.rs` build script"),
            TracersError::BuildInfoReadError { message, .. } => write!(f, "{}", message),
            TracersError::BuildInfoWriteError { message, .. } => write!(f, "{}", message),
            TracersError::CodeGenerationError { message } => write!(f, "Error generating probing code: {}", message)
        }
    }
}

unsafe impl Send for TracersError {}
unsafe impl Sync for TracersError {}

impl PartialEq<TracersError> for TracersError {
    fn eq(&self, other: &TracersError) -> bool {
        //There are a lot of types that don't support equality.  To keep it easy, just compare the
        //display version
        self.to_string() == other.to_string()
    }
}

impl TracersError {
    pub fn invalid_provider<T: ToTokens>(message: impl AsRef<str>, element: T) -> TracersError {
        let message = format!(
            "There's a problem with this provider trait: {}",
            message.as_ref()
        );
        let e = Self::new_syn_error(&message, element);

        TracersError::InvalidProvider {
            message,
            syn_error: e,
        }
    }

    pub fn syn_error(message: impl AsRef<str>, e: syn::Error) -> TracersError {
        // When this happens, it means we got a `syn::Error` back from the `syn` library when we
        // asked it to parse a token stream.  The message in this error will be something generic,
        // where as our `message` will have important details that will help the user understand
        // what went wrong.  So we need to construct our own `syn::Error` which includes this
        // context, while using the `span` from `e` so it is attached to the right part of the
        // input code.
        let message = format!("Parse error: {}\nDetails: {}", message.as_ref(), e);
        let e = syn::Error::new(e.span(), &message);
        let e = Self::wrap_syn_error(e);

        TracersError::SynError {
            message,
            syn_error: e,
        }
    }

    pub fn invalid_call_expression<T: ToTokens>(
        message: impl AsRef<str>,
        element: T,
    ) -> TracersError {
        let message = format!("Invalid call expression: {}", message.as_ref());
        let e = Self::new_syn_error(&message, element);
        TracersError::InvalidCallExpression {
            message,
            syn_error: e,
        }
    }

    pub fn other_error<D: Display + Send + Sync + 'static>(e: failure::Context<D>) -> TracersError {
        TracersError::OtherError {
            message: Self::fail_string(&e),
            error: e.into(),
        }
    }

    pub fn missing_call_in_build_rs() -> TracersError {
        TracersError::MissingCallInBuildRs
    }

    pub fn build_info_read_error(build_info_path: PathBuf, e: Error) -> TracersError {
        let message = format!("Unable to read build info from '{}'.\nAre you sure you're calling `tracers_build::build()` in your `build.rs`?\nError cause: {}",
            build_info_path.display(),
            Self::error_string(&e));
        TracersError::BuildInfoReadError {
            message,
            build_info_path: build_info_path.display().to_string(),
            error: e,
        }
    }

    pub fn build_info_write_error(build_info_path: PathBuf, e: Error) -> TracersError {
        let message = format!("Unable to write build info from '{}'.\nAre you sure you're calling `tracers_build::build()` in your `build.rs`?\nError cause: {}",
            build_info_path.display(),
            Self::error_string(&e));
        TracersError::BuildInfoWriteError {
            message,
            build_info_path: build_info_path.display().to_string(),
            error: e,
        }
    }

    pub fn code_generation_error<S: AsRef<str>>(message: S) -> TracersError {
        TracersError::CodeGenerationError {
            message: message.as_ref().to_owned(),
        }
    }

    /// Converts this error type into a `syn::Error`, preserving context from spans and elements if
    /// any were given
    pub fn into_syn_error(self) -> syn::Error {
        match self {
            TracersError::InvalidProvider { syn_error, .. } => Self::error_as_syn_error(syn_error),
            TracersError::SynError { syn_error, .. } => Self::error_as_syn_error(syn_error),
            TracersError::InvalidCallExpression { syn_error, .. } => {
                Self::error_as_syn_error(syn_error)
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

    fn new_syn_error<T: ToTokens, U: Display>(message: U, tokens: T) -> Error {
        Self::wrap_syn_error(syn::Error::new_spanned(tokens, message))
    }

    fn wrap_syn_error(e: syn::Error) -> Error {
        Self::wrap_error(e)
    }

    /// Given a `failure` `Error` type, tests to see if it wraps a real `syn::Error`, and if it
    /// doesn't, creates a `syn::Error` with the same message
    fn error_as_syn_error(e: Error) -> syn::Error {
        e.downcast::<syn::Error>()
            .unwrap_or_else(|e| syn::Error::new(Span::call_site(), e.to_string()))
    }

    fn wrap_error(e: impl std::error::Error + Sync + Send + 'static) -> Error {
        e.into()
    }

    /// Builds an error string with all the relevant info from a `Fail` implementation
    #[allow(clippy::redundant_closure)] //clippy's proposed alterstatic won't compile here
    fn error_string(e: &Error) -> String {
        let causes: Vec<_> = e.iter_chain().map(|c| c.to_string()).collect();
        causes.join(": ")
    }

    #[allow(clippy::redundant_closure)] //clippy's proposed alterstatic won't compile here
    fn fail_string(f: &dyn Fail) -> String {
        let causes: Vec<_> = f.iter_chain().map(|c| c.to_string()).collect();
        causes.join(": ")
    }
}

/// Implement conversion from regular errors into a TracersError, but only if the error has been
/// given a context message using the `.context()` extension method provided by `failure`
impl<D: Display + Send + Sync + 'static> From<failure::Context<D>> for TracersError {
    fn from(failure: failure::Context<D>) -> TracersError {
        TracersError::other_error(failure)
    }
}

pub type TracersResult<T> = std::result::Result<T, TracersError>;
