//! This module generates the code for a call to a probe implemented with the `libstapsdt` library.
//! It's rather simple, because it assumes the Rust bindings on the `libstapsdt` API are already a
//! dependency and exposed via the `SystemTracer` type alias.

use crate::spec::ProbeCallSpecification;
use crate::ProberResult;
use proc_macro2::TokenStream;
use quote::quote_spanned;
use syn::spanned::Spanned;

pub(super) fn generate_probe_call(call: ProbeCallSpecification) -> ProberResult<TokenStream> {
    match call {
        ProbeCallSpecification::FireOnly(details) => {
            //The no-op implementation doesn't permute the probe name at all, it's just called
            //directly
            let span = details.call.span();
            let call = &details.call;
            Ok(quote_spanned! {span=>
                if false {
                    //We call into the original probe method, which has a `#[deprecated]` attribute
                    //on it to help users who mistakenly call the probe method directly.  We can
                    //safely suppress that warning here
                    #[allow(deprecated)]
                    #call;
                }
            })
        }
        ProbeCallSpecification::FireWithCode { .. } => unimplemented!(),
    }
}
