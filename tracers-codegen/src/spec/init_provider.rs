//! This module parses the tokens passed to the `init_provider!` macro, validates them, and
//! represents the tokens in a form that generators can easily make use of
use crate::syn_helpers;
use crate::{TracersError, TracersResult};
use proc_macro2::TokenStream;
use std::fmt;

/// This is probably the simplest of the tracers specs.  Callers use the `init_provider!` macro to
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
/// decorated with the `tracer` attribute.  It's realy that simple.
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
    pub fn from_token_stream(tokens: TokenStream) -> TracersResult<ProviderInitSpecification> {
        match syn::parse2::<syn::Path>(tokens) {
            Ok(path) => Self::from_path(path),
            Err(e) => Err(TracersError::syn_error("Expected a type path", e)),
        }
    }

    pub fn from_path(path: syn::Path) -> TracersResult<ProviderInitSpecification> {
        Ok(ProviderInitSpecification { provider: path })
    }
}
