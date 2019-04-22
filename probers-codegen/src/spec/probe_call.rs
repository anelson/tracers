//! This module handles parsing calls to a probe made using the `probe!` proc macro, decomposing
//! them into the discrete components, and validating the call is syntactically valid.
use crate::syn_helpers;
use crate::{ProbersError, ProbersResult};
use proc_macro2::TokenStream;
use std::fmt;

/// There are two kinds of probe calls:
///
/// The simple one looks like this:
///
/// ```no_execute
/// probe!(MyProvider::my_probe(arg0, arg1, arg2));
/// ```
///
/// That is a `FireOnly` call.
///
/// The more complex one includes one or more additional statements, which should only be evaluated
/// if the probe is actually enabled.  Not just enabled at compile time, but if at runtime the
/// probe is activated.  That looks like this:
///
/// ```no_execute
/// probe! {
///     println!("This probe is enabled!");
///
///     let stats = some_expensive_func();
///
///     MyProvider::my_probe(arg0, arg1, stats);
/// }
/// ```
///
/// This is a `FireWithCode` call.
///
/// Either call can be made on a probe.  Probes are not explicitly one kind or the other; the
/// difference is only in how they are fired.
#[derive(Debug, PartialEq)]
pub enum ProbeCallSpecification {
    FireOnly(ProbeCallDetails),
    FireWithCode {
        call: ProbeCallDetails,
        statements: syn::Block,
    },
}

impl ProbeCallSpecification {
    pub fn from_token_stream(tokens: TokenStream) -> ProbersResult<ProbeCallSpecification> {
        //TODO: Also try matching on a Block expression to support the `FireWithCode` variation
        match syn::parse2::<syn::Expr>(tokens) {
            Ok(call) => {
                ProbeCallDetails::from_call_expression(call).map(ProbeCallSpecification::FireOnly)
            }
            Err(e) => Err(ProbersError::syn_error(
                "Expecting a Rust function call expression",
                e,
            )),
        }
    }
}

/// Contains all of the details of an invocation of a probe, already decomposed for the generators
/// to work with
#[derive(PartialEq)]
pub struct ProbeCallDetails {
    pub call: syn::ExprCall,
    pub probe_fq_path: syn::Path,
    pub provider: syn::Path,
    pub probe: syn::PathSegment,
    pub args: Vec<syn::Expr>,
}

impl fmt::Debug for ProbeCallDetails {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ProbeCallDetails(")?;
        write!(f, "call={}", syn_helpers::convert_to_string(&self.call))?;
        write!(
            f,
            ", probe_fq_path={}",
            syn_helpers::convert_to_string(&self.probe_fq_path)
        )?;
        write!(
            f,
            ", provider={}",
            syn_helpers::convert_to_string(&self.provider)
        )?;
        write!(f, ", probe={}", syn_helpers::convert_to_string(&self.probe))?;
        write!(f, ", args=[")?;

        let args: Vec<_> = self
            .args
            .iter()
            .map(|arg| format!("[{}], ", syn_helpers::convert_to_string(arg)))
            .collect();

        write!(f, "{}", args.join(", "))?;

        write!(f, "])")
    }
}

impl ProbeCallDetails {
    /// Parses a single expression which should be a call to the probe function.  This implies that
    /// the probe call is `FireOnly`, since a block of code would take a different form.
    pub fn from_call_expression(call: syn::Expr) -> ProbersResult<ProbeCallDetails> {
        match call {
            syn::Expr::Call(call) => {
                // Within this call is encoded all the information we need about the probe firing,
                // we just have to extract it
                let func = call.func.as_ref().clone();

                if let syn::Expr::Path(func) = func {
                    if func.path.segments.len() < 2 {
                        return Err(
                            ProbersError::invalid_call_expression(format!(
                            "The expression '{0}' is missing the name of the provider trait, eg 'MyProviderTrait::{0}'",
                            syn_helpers::convert_to_string(&call)),
                                    func));
                    }

                    let mut provider = func.path.clone();

                    //For paths of the form "foo::bar", when we call 'pop', we get back 'bar' and
                    //the remaining path is "foo::".  So pop the last path element off to get the
                    //probe name, then forcibly override that trailing :: separator
                    let (probe, _) = provider.segments.pop().unwrap().into_tuple(); //all call expressions have at least one segment
                    if provider.segments.trailing_punct() {
                        let pair = provider.segments.pop().unwrap(); //trailing_punct is true so there's at least one segmnet

                        match pair {
                            syn::punctuated::Pair::Punctuated(seg, _) => {
                                provider.segments.push_value(seg)
                            }
                            syn::punctuated::Pair::End(seg) => provider.segments.push_value(seg),
                        }
                    }

                    let args: Vec<_> = call.args.iter().cloned().collect();
                    Ok(ProbeCallDetails {
                        call,
                        probe_fq_path: func.path,
                        provider,
                        probe,
                        args,
                    })
                } else {
                            return Err(ProbersError::invalid_call_expression(format!(
                            "Unexpected expression for function call: {}",
                            syn_helpers::convert_to_string(&func)),
                                    func));
                }
            },
            _ => {
                Err(ProbersError::invalid_call_expression( "The probe! macro requires the name of a provider trait and its probe method, e.g. MyProvider::myprobe(...)", call))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::testdata::*;

    #[test]
    fn parses_all_test_cases() {
        for test_call in get_test_probe_calls().into_iter() {
            let call_str = syn_helpers::convert_to_string(&test_call.call);
            match test_call.expected {
                Ok(expected_call) => assert_eq!(
                    expected_call,
                    ProbeCallSpecification::from_token_stream(test_call.call).expect(&format!(
                        "Unexpected error parsing probe call: '{}'",
                        call_str
                    ))
                ),
                Err(error_msg) => match ProbeCallSpecification::from_token_stream(test_call.call) {
                    Ok(_) => panic!(
                        "Probe call '{}' should have failed to parse but instead it succeeded",
                        call_str
                    ),
                    Err(e) => assert!(
                        e.to_string().contains(error_msg),
                        "Expected substring '{}' in error message '{}'",
                        error_msg,
                        e.to_string()
                    ),
                },
            }
        }
    }
}
