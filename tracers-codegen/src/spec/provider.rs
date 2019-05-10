//! This module provides functionality to scan the AST of a Rust source file and identify
//! `tracers` provider traits therein, as well as analyze those traits and produce `ProbeSpec`s for
//! each of the probes they contain.  Once the provider traits have been discovered, other modules
//! in this crate can then process them in various ways
use crate::hashing::HashCode;
use crate::serde_helpers;
use crate::spec::ProbeSpecification;
use crate::{TracersError, TracersResult};
use darling::FromMeta;
use heck::SnakeCase;
use proc_macro2::TokenStream;
use quote::quote;
use serde::{Deserialize, Serialize};
use std::fmt;
use syn::parse::{Parse, ParseStream, Result as ParseResult};
use syn::visit::Visit;
use syn::Token;
use syn::{ItemTrait, TraitItem};

/// Struct which contains the parsed and processed contents of the `tracer` attribute.
#[derive(Debug, FromMeta, Clone, Serialize, Deserialize)]
pub(crate) struct TracerArgs {
    #[darling(default)]
    provider_name: Option<String>,
}

/// Implement parsing the arguments portion of a `#[tracer]` attribute.  This does _not_ parse the
/// whole attribute.  It parses only the part after the `tracer` name, which contains optional
/// parameters that control the behavior of the generated provider.
impl Parse for TracerArgs {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        //This implementation mostly copied from
        //https://github.com/dtolnay/syn/blob/master/src/parse_macro_input.rs which for some reason
        //is hidden and only works with `proc_macro` not `proc_macro2`
        let mut metas: Vec<syn::NestedMeta> = vec![];

        loop {
            if input.is_empty() {
                break;
            }
            let value = input.parse()?;
            metas.push(value);
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }

        TracerArgs::from_list(&metas).map_err(|e| input.error(e))
    }
}

impl TracerArgs {
    pub(crate) fn from_token_stream(attr: TokenStream) -> TracersResult<Self> {
        syn::parse2(attr).map_err(|e| TracersError::syn_error("Error parsing attribute args", e))
    }

    pub(crate) fn from_attribute(attr: syn::Attribute) -> TracersResult<Self> {
        Self::from_token_stream(attr.tts)
    }
}

/// A bit of a hack to work around a limitation in the `syn` crate.  It doesn't expose an API to
/// parse an attribute directly; it expects you to implement the `Parse` trait to do it yourself.
/// So this does that, though it assumes it's only ever used on a `#[tracer]` attribute because it
/// attempts to parse possible `tracer` parameters in the attribute`
pub(crate) struct TracerAttribute {
    args: TracerArgs,
}

impl TracerAttribute {
    fn from_attribute(attr: syn::Attribute) -> TracersResult<Self> {
        Ok(TracerAttribute {
            args: TracerArgs::from_attribute(attr)?,
        })
    }
}

impl Parse for TracerAttribute {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        let mut attrs: Vec<syn::Attribute> = input.call(syn::Attribute::parse_outer)?;

        if let Some(tracer_attr) = attrs.pop() {
            Ok(TracerAttribute {
                args: syn::parse2(tracer_attr.tts)?,
            })
        } else {
            return Err(input.error("Expected exactly one attribute, `#[tracer]`"));
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ProviderSpecification {
    name: String,
    hash: HashCode,
    #[serde(with = "serde_helpers::syn")]
    item_trait: ItemTrait,
    #[serde(with = "serde_helpers::token_stream")]
    token_stream: TokenStream,
    args: TracerArgs,
    probes: Vec<ProbeSpecification>,
}

impl fmt::Debug for ProviderSpecification {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "ProviderSpecification(
    name='{}',
    probes:",
            self.name
        )?;

        for probe in self.probes.iter() {
            writeln!(f, "        {:?},", probe)?;
        }

        write!(f, ")")
    }
}

impl ProviderSpecification {
    fn new(args: TracerArgs, item_trait: ItemTrait) -> TracersResult<ProviderSpecification> {
        let probes = find_probes(&item_trait)?;
        let token_stream = quote! { #item_trait };
        let hash = crate::hashing::hash(&item_trait);
        Ok(ProviderSpecification {
            name: Self::provider_name_from_trait(&item_trait.ident),
            hash,
            item_trait,
            token_stream,
            args,
            probes,
        })
    }

    pub(crate) fn from_token_stream(
        args: TracerArgs,
        tokens: TokenStream,
    ) -> TracersResult<ProviderSpecification> {
        match syn::parse2::<syn::ItemTrait>(tokens) {
            Ok(item_trait) => Self::new(args, item_trait),
            Err(e) => Err(TracersError::syn_error("Expected a trait", e)),
        }
    }

    pub(crate) fn from_trait(
        attr: TracerAttribute,
        item_trait: ItemTrait,
    ) -> TracersResult<ProviderSpecification> {
        Self::new(attr.args, item_trait)
    }

    /// Computes the name of a provider given the name of the provider's trait.
    ///
    pub(crate) fn provider_name_from_trait(ident: &syn::Ident) -> String {
        // The provider name must be chosen carefully.  As of this writing (2019-04) the `bpftrace`
        // and `bcc` tools have, shall we say, "evolving" support for USDT.  As of now, with the
        // latest git version of `bpftrace`, the provider name can't have dots or colons.  For now,
        // then, the provider name is just the name of the provider trait, converted into
        // snake_case for consistency with USDT naming conventions.  If two modules in the same
        // process have the same provider name, they will conflict and some unspecified `bad
        // things` will happen.
        ident.to_string().to_snake_case()
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// The name of this provider (in snake_case) combined with the hash of the provider's
    /// contents.  Eg: `my_provider_deadc0de1918df`
    pub(crate) fn name_with_hash(&self) -> String {
        format!("{}_{:x}", self.name, self.hash)
    }

    pub(crate) fn hash(&self) -> HashCode {
        self.hash
    }

    pub(crate) fn ident(&self) -> &syn::Ident {
        &self.item_trait.ident
    }

    pub(crate) fn item_trait(&self) -> &syn::ItemTrait {
        &self.item_trait
    }

    pub(crate) fn token_stream(&self) -> &TokenStream {
        &self.token_stream
    }

    pub(crate) fn probes(&self) -> &Vec<ProbeSpecification> {
        &self.probes
    }

    /// Consumes this spec and returns the same spec with all probes removed, and instead the
    /// probes vector is returned separately.  This is a convenient way to wrap
    /// ProviderSpecification in something else (in truth its designed for the
    /// `ProviderTraitGenerator` implementation classes)
    pub(crate) fn separate_probes(self) -> (ProviderSpecification, Vec<ProbeSpecification>) {
        let probes = self.probes;
        (
            ProviderSpecification {
                name: self.name,
                hash: self.hash,
                item_trait: self.item_trait,
                token_stream: self.token_stream,
                args: self.args,
                probes: Vec::new(),
            },
            probes,
        )
    }
}

/// Scans the AST of a Rust source file, finding all traits marked with the `tracer` attribute,
/// parses the contents of the trait, and deduces the provider spec from that.
///
/// Note that if any traits are encountered with the `tracer` attribute but which are in some way
/// invalid as providers, those traits will be silently ignored.  At compile time the `tracer`
/// attribute will cause a very detailed compile error so there's no chance the user will miss this
/// mistake.
pub(crate) fn find_providers(ast: &syn::File) -> Vec<ProviderSpecification> {
    //Construct an implementation of the `syn` crate's `Visit` trait which will examine all trait
    //declarations in the file looking for possible providers
    struct Visitor {
        providers: Vec<ProviderSpecification>,
    }

    impl<'ast> Visit<'ast> for Visitor {
        fn visit_item_trait(&mut self, i: &'ast ItemTrait) {
            //First pass through to the default impl
            syn::visit::visit_item_trait(self, i);

            fn is_tracer_attribute(attr: &syn::Attribute) -> bool {
                match attr.path.segments.iter().last() {
                    Some(syn::PathSegment { ident, .. }) if *ident == "tracer" => true,
                    _ => false,
                }
            }

            //Check for the `tracer` or `tracers::tracer` attribute, splitting it out from the rest
            //of the attributes if present to ensure the
            //hash matches the same hash computed by the proc macro when it's invoked on this
            //trait during the compile stage
            let mut i = i.clone();
            let (mut tracer_attrs, other_attrs) = i
                .attrs
                .into_iter()
                .partition::<Vec<syn::Attribute>, _>(is_tracer_attribute);
            if let Some(tracer_attr) = tracer_attrs.pop() {
                //This looks like a provider trait.
                //Normally there should be only one `#[tracer]` but that's for the full compiler to
                //sort out.  We'll just assume the first one is the one we want
                i.attrs = other_attrs;
                if let Ok(provider) = ProviderSpecification::from_trait(
                    TracerAttribute::from_attribute(tracer_attr)
                        .expect("Failed parsing attribute metadata"),
                    i,
                ) {
                    self.providers.push(provider)
                }
            }
        }
    }

    let mut visitor = Visitor {
        providers: Vec::new(),
    };
    visitor.visit_file(ast);

    visitor.providers
}

/// Looking at the methods defined on the trait, deduce from those methods the probes that we will
/// need to define, including their arg counts and arg types.
///
/// If the trait contains anything other than method declarations, or any of the declarations are
/// not suitable as probes, an error is returned
fn find_probes(item: &ItemTrait) -> TracersResult<Vec<ProbeSpecification>> {
    if item.generics.type_params().next() != None || item.generics.lifetimes().next() != None {
        return Err(TracersError::invalid_provider(
            "Probe traits must not take any lifetime or type parameters",
            item,
        ));
    }

    // Look at the methods on the trait and translate each one into a probe specification
    let mut specs: Vec<ProbeSpecification> = Vec::new();
    for f in item.items.iter() {
        match f {
            TraitItem::Method(ref m) => {
                specs.push(ProbeSpecification::from_method(item, m)?);
            }
            _ => {
                return Err(TracersError::invalid_provider(
                    "Probe traits must consist entirely of methods, no other contents",
                    f,
                ));
            }
        }
    }

    Ok(specs)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::testdata::*;
    use std::io::{BufReader, BufWriter};
    use syn::parse_quote;

    impl PartialEq<ProviderSpecification> for ProviderSpecification {
        fn eq(&self, other: &ProviderSpecification) -> bool {
            self.name == other.name && self.probes == other.probes
        }
    }

    /// Allows tests to compare a test case directly to a ProviderSpecification to ensure they match
    impl PartialEq<TestProviderTrait> for ProviderSpecification {
        fn eq(&self, other: &TestProviderTrait) -> bool {
            self.name == other.provider_name
                && other
                    .probes
                    .as_ref()
                    .map(|probes| &self.probes == probes)
                    .unwrap_or(false)
        }
    }

    fn get_filtered_test_traits(with_errors: bool) -> Vec<TestProviderTrait> {
        get_test_provider_traits(|t: &TestProviderTrait| t.expected_error.is_some() == with_errors)
    }

    #[test]
    fn find_providers_ignores_invalid_traits() {
        for test_trait in get_filtered_test_traits(true) {
            let trait_decl = test_trait.tokenstream;
            let test_file: syn::File = parse_quote! {
                #[tracer]
                #trait_decl
            };

            assert_eq!(
                None,
                find_providers(&test_file).first(),
                "The invalid trait '{}' was returned by find_providers as valid",
                test_trait.description
            );
        }
    }

    #[test]
    fn find_providers_finds_valid_traits() {
        for test_trait in get_filtered_test_traits(false) {
            let trait_attr = test_trait.attr_tokenstream.clone();
            let trait_decl = test_trait.tokenstream.clone();
            let test_file: syn::File = parse_quote! {
                #trait_attr
                #trait_decl
            };

            let mut providers = find_providers(&test_file);
            assert_ne!(
                0,
                providers.len(),
                "the test trait '{}' was not properly detected by find_provider",
                test_trait.description
            );

            assert_eq!(providers.pop().unwrap(), test_trait);
        }
    }

    #[test]
    fn find_probes_fails_with_invalid_traits() {
        for test_trait in get_filtered_test_traits(true) {
            let trait_decl = test_trait.tokenstream;
            let item_trait: syn::ItemTrait = parse_quote! {
                #[tracer]
                #trait_decl
            };

            let error = find_probes(&item_trait).err();
            assert_ne!(
                None, error,
                "The invalid trait '{}' was returned by find_probes as valid",
                test_trait.description
            );

            let expected_error_substring = test_trait.expected_error.unwrap();
            let message = error.unwrap().to_string();
            assert!(message.contains(expected_error_substring),
                "The invalid trait '{}' should produce an error containing '{}' but instead it produced '{}'",
                test_trait.description,
                expected_error_substring,
                message
            );
        }
    }

    #[test]
    fn find_probes_succeeds_with_valid_traits() {
        for test_trait in get_filtered_test_traits(false) {
            let trait_decl = test_trait.tokenstream;
            let item_trait: syn::ItemTrait = parse_quote! {
                #[tracer]
                #trait_decl
            };

            let probes = find_probes(&item_trait).unwrap();
            assert_eq!(probes, test_trait.probes.unwrap_or(Vec::new()));
        }
    }

    #[test]
    fn found_providers_have_same_hash() {
        //There are two ways for us to get a ProviderSpecification:
        // The first is from a TokenStream in an attribute proc macro.  That's how the `#[tracer]`
        // macro works.
        // The second is in calling `find_providers` to discover providers in a source file
        //
        // A fundamental assumption made in this project is that the hashes of a given provider
        // will be identical regardless of which of the two ways it was obtained.
        //
        // However, the attribute proc macro gets a token stream which does not include the
        // `#[tracer]` attribute itself.  The `find_providers` code must see this attribute because
        // that's how it identifies a provider trait.  This test ensures the hashes are always
        // corrected to match
        for test_trait in get_filtered_test_traits(false) {
            let trait_decl = test_trait.tokenstream;

            //Note: we remove the `#[tracer]` attribute from the token stream because that's what
            //the proc macro infrastructure does.  All other attributes are preserved, but the
            //attribute that triggers the macro is provided in a separate token stream which we
            //ignore
            let token_stream = quote! {
                /// This is a provider trait
                #trait_decl
            };
            let provider_from_ts = ProviderSpecification::from_trait(
                syn::parse2(test_trait.attr_tokenstream).unwrap(),
                syn::parse2(token_stream).unwrap(),
            )
            .unwrap();
            let file: syn::File = parse_quote! {
                mod foo {
                    fn useless_func() -> bool { false }
                }

                trait NotAProvider {
                    fn probe0(not_a_probe_arg: usize);
                }

                /// This is a provider trait
                #[tracer]
                #trait_decl
            };

            let providers = find_providers(&file);

            assert_eq!(1, providers.len());
            let provider_from_file = providers.get(0).unwrap();

            assert_eq!(provider_from_ts.name(), provider_from_file.name());
            assert_eq!(provider_from_ts.hash(), provider_from_file.hash());
        }
    }

    #[test]
    fn provider_serde_test() {
        //Go through all of the valid test traits, parse them in to a provider, then serialize and
        //deserialize to json to make sure the round trip serialization works
        for test_trait in get_filtered_test_traits(false) {
            println!("Parsing attribute: {}", test_trait.attr_tokenstream);
            let (attr, item_trait) = test_trait.get_attr_and_item_trait();
            let provider = ProviderSpecification::from_trait(attr, item_trait).unwrap();
            let mut buffer = Vec::new();
            let writer = BufWriter::new(&mut buffer);
            serde_json::to_writer(writer, &provider).unwrap();

            let reader = BufReader::new(buffer.as_slice());

            let rt_provider: ProviderSpecification = match serde_json::from_reader(reader) {
                Ok(p) => p,
                Err(e) => {
                    panic!(
                        r###"Error deserializing provider:
                            Test case: {}
                            JSON: {}
                            Error: {}"###,
                        test_trait.description,
                        String::from_utf8(buffer).unwrap(),
                        e
                    );
                }
            };

            assert_eq!(
                provider, rt_provider,
                "test case: {}",
                test_trait.description
            );
        }
    }

    #[test]
    fn parses_tracer_attributes_test() {
        //Make sure the `#[tracer]` attribute on all valid test cases can parse correctly
        for test_trait in get_filtered_test_traits(false) {
            println!("Parsing attribute: {}", test_trait.attr_tokenstream);
            let _attribute: TracerAttribute =
                syn::parse2(test_trait.attr_tokenstream).expect("Expected valid tracer attribute");
        }
    }
}
