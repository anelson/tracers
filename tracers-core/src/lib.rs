#![deny(warnings)]

#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;

//Re-export some third-party dependencies so the caller can be sure to use the exact version we use
//and doesn't have to add their own explicit dep
pub extern crate failure;

pub mod argtypes;
pub use argtypes::{wrap, ProbeArgNativeType, ProbeArgType, ProbeArgWrapper};

/// The result of a provider init is either a string with some free-form details about the
/// provider, or a string indicating the error which prevented the provider from initializing
///
/// On success, the string takes the form:
///
/// ```not_rust
/// $PROVIDER_NAME::$IMPLEMENTATION::$VERSION
/// ```
///
/// for example:
///
/// ```not_rust
/// my_provider::native/native_noop::0.1.0
/// ```
pub type ProviderInitResult = std::result::Result<&'static str, &'static str>;

#[cfg(feature = "dynamic")]
pub mod dynamic;
