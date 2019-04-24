extern crate proc_macro;

//We have to use the `proc_macro` types for the actual macro impl, but everywhere else we'll use
//`proc_macro2` for better testability
use tracers_codegen::proc_macros::{init_provider_impl, probe_impl, tracer_impl};
use proc_macro::TokenStream as CompilerTokenStream;
use proc_macro2::TokenStream;
use proc_macro_hack::proc_macro_hack;

#[proc_macro_hack]
pub fn probe(input: CompilerTokenStream) -> CompilerTokenStream {
    match probe_impl(TokenStream::from(input)) {
        Ok(stream) => stream,
        Err(err) => err.into_compiler_error(),
    }
    .into()
}

#[proc_macro_hack]
pub fn init_provider(input: CompilerTokenStream) -> CompilerTokenStream {
    match init_provider_impl(TokenStream::from(input)) {
        Ok(stream) => stream,
        Err(err) => err.into_compiler_error(),
    }
    .into()
}

#[proc_macro_attribute]
pub fn tracer(_attr: CompilerTokenStream, item: CompilerTokenStream) -> CompilerTokenStream {
    match tracer_impl(TokenStream::from(item)) {
        Ok(stream) => stream,
        Err(err) => err.into_compiler_error(),
    }
    .into()
}
