extern crate proc_macro;

//We have to use the `proc_macro` types for the actual macro impl, but everywhere else we'll use
//`proc_macro2` for better testability
use probers_codegen::proc_macros::{init_provider_impl, probe_impl, prober_impl};
use proc_macro::TokenStream as CompilerTokenStream;
use proc_macro2::Span;
use proc_macro2::TokenStream;
use proc_macro_hack::proc_macro_hack;
use quote::quote_spanned;

#[proc_macro_hack]
pub fn probe(input: CompilerTokenStream) -> CompilerTokenStream {
    match probe_impl(TokenStream::from(input)) {
        Ok(stream) => stream,
        Err(err) => report_error(&err.message, err.span),
    }
    .into()
}

#[proc_macro_hack]
pub fn init_provider(input: CompilerTokenStream) -> CompilerTokenStream {
    match init_provider_impl(TokenStream::from(input)) {
        Ok(stream) => stream,
        Err(err) => report_error(&err.message, err.span),
    }
    .into()
}

#[proc_macro_attribute]
pub fn prober(_attr: CompilerTokenStream, item: CompilerTokenStream) -> CompilerTokenStream {
    match prober_impl(TokenStream::from(item)) {
        Ok(stream) => stream,
        Err(err) => report_error(&err.message, err.span),
    }
    .into()
}

/// Reports a compile error in our macro, which is then reported to the user via the
/// `compile_error!` macro injected into the token stream.  Cool idea stolen from
/// https://internals.rust-lang.org/t/custom-error-diagnostics-with-procedural-macros-on-almost-stable-rust/8113
fn report_error(msg: &str, span: Span) -> TokenStream {
    //NB: When the unstable feature `proc_macro_diagnostic` is stabilized, use that instead of this
    //hack
    //
    //span.unwrap().error(msg).emit();
    //TokenStream::new()
    quote_spanned! {span=>
        compile_error! { #msg }
    }
}
