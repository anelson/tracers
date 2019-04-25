//! This generates probe calls for the `probe!` macro when the no-op implementation is being used.
//! That means the call doesn't do anything at runtime, but it should still include in the code an
//! unreachable line that performs the call, just to make sure the compiler still does type
//! checking and counts the arguments as being used.

use crate::spec::ProbeCallSpecification;
use crate::TracersResult;
use proc_macro2::TokenStream;
use quote::quote_spanned;
use syn::spanned::Spanned;

pub(crate) fn generate_probe_call(call: ProbeCallSpecification) -> TracersResult<TokenStream> {
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
