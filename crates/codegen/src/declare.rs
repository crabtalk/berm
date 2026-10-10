//! `declare_program!` — typed calls, types and events from another program's
//! IDL.
//!
//! The IDL is the JSON `berm idl` writes: a deployed program as the control API
//! reports it. It is read from `idls/{name}.json` in the crate's directory or
//! the nearest ancestor that has one.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};
use syn::Ident;

pub fn expand(name: &Ident) -> syn::Result<TokenStream> {
    let path = find(name)?;
    let text = std::fs::read_to_string(&path)
        .map_err(|error| syn::Error::new(name.span(), format!("{}: {error}", path.display())))?;
    let idl: Value = serde_json::from_str(&text)
        .map_err(|error| syn::Error::new(name.span(), format!("{}: {error}", path.display())))?;
    let fail =
        |message: &str| syn::Error::new(name.span(), format!("{}: {message}", path.display()));

    let program = idl["name"].as_str().ok_or_else(|| fail("no `name`"))?;
    let digest = idl["digest"].as_str().unwrap_or_default();
    let tools = idl["tools"].as_array().ok_or_else(|| fail("no `tools`"))?;
    let events = idl["events"].as_array().cloned().unwrap_or_default();

    // Every titled object anywhere in the IDL is one struct, by its title.
    let mut structs = BTreeMap::new();
    for tool in tools {
        collect(&tool["parameters"], &mut structs);
        collect(&tool["returns"], &mut structs);
    }
    for event in &events {
        collect(&event["fields"], &mut structs);
    }

    let types = structs.iter().map(|(title, schema)| {
        let ident = format_ident!("{title}");
        let doc = schema["description"]
            .as_str()
            .map(|text| quote!(#[doc = #text]));
        let required: Vec<&str> = schema["required"]
            .as_array()
            .map(|names| names.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let fields =
            schema["properties"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(field, property)| {
                    let ident = field_ident(field);
                    let ty = rust_type(property, &quote!());
                    let doc = property["description"]
                        .as_str()
                        .map(|text| quote!(#[doc = #text]));
                    match required.contains(&field.as_str()) {
                        true => quote! {
                            #doc
                            #[serde(rename = #field)]
                            pub #ident: #ty,
                        },
                        false => quote! {
                            #doc
                            #[serde(
                                rename = #field,
                                default,
                                skip_serializing_if = "::core::option::Option::is_none"
                            )]
                            pub #ident: ::core::option::Option<#ty>,
                        },
                    }
                });
        quote! {
            #doc
            #[derive(
                Debug,
                Clone,
                ::berm_lang::serde::Serialize,
                ::berm_lang::serde::Deserialize,
            )]
            #[serde(crate = "::berm_lang::serde")]
            pub struct #ident {
                #(#fields)*
            }
        }
    });

    let names: Vec<Ident> = events
        .iter()
        .filter_map(|event| event["name"].as_str())
        .map(|name| format_ident!("{name}"))
        .collect();
    let event_docs = events
        .iter()
        .map(|event| event["description"].as_str().unwrap_or_default());
    let reads = names.iter().map(|name| {
        quote! {
            if let ::core::option::Option::Some(read) = logged.read::<#name>() {
                return ::core::option::Option::Some(read.map(Self::#name));
            }
        }
    });
    let events = match names.is_empty() {
        true => quote!(),
        false => quote! {
            /// The events it emits, each a log under its name.
            pub mod events {
                pub use super::types::{#(#names),*};

                #(
                    impl ::berm_lang::Event for #names {
                        const NAME: &'static str = stringify!(#names);
                    }
                )*

                /// Any one of its events.
                #[derive(Debug, Clone)]
                pub enum Event {
                    #(#[doc = #event_docs] #names(#names),)*
                }

                impl Event {
                    /// `logged` as one of these events, or `None` when its
                    /// topics name none of them.
                    pub fn read(
                        logged: &::berm_lang::Logged,
                    ) -> ::core::option::Option<
                        ::core::result::Result<Self, ::berm_lang::serde_json::Error>,
                    > {
                        #(#reads)*
                        ::core::option::Option::None
                    }
                }
            }
        },
    };

    let calls = tools.iter().map(|tool| {
        let tool_name = tool["name"].as_str().unwrap_or_default();
        let ident = field_ident(tool_name);
        let doc = tool["description"].as_str().unwrap_or_default();
        let types = quote!(super::types::);
        let (args, encode) = match tool["parameters"]["title"].as_str() {
            Some(_) => {
                let ty = rust_type(&tool["parameters"], &types);
                (
                    quote!(args: &#ty),
                    quote!(let args = ::berm_lang::json::to(args)?;),
                )
            }
            None => (quote!(args: &str), quote!()),
        };
        let (returns, decode) = match tool["returns"]["title"].as_str() {
            Some(_) => (
                rust_type(&tool["returns"], &types),
                quote!(::berm_lang::json::from(&result)),
            ),
            None => (
                quote!(::berm_lang::__private::String),
                quote!(::core::result::Result::Ok(result)),
            ),
        };
        quote! {
            #[doc = #doc]
            pub fn #ident(#args) -> ::core::result::Result<#returns, ::berm_lang::CallError> {
                #encode
                let result = ::berm_lang::call(super::NAME, #tool_name, &args)?;
                #decode
            }
        }
    });

    let module = name;
    let source = path.display().to_string();
    Ok(quote! {
        pub mod #module {
            // Rebuilds this crate when the IDL changes.
            const _: &str = include_str!(#source);

            /// What the program is deployed as, which every call names.
            pub const NAME: &str = #program;
            /// sha256 of the image the IDL was read from.
            pub const DIGEST: &str = #digest;

            /// Its argument, result and event structs.
            pub mod types {
                #(#types)*
            }

            #events

            /// Its tools, each a `berm_lang::call` with typed arguments and
            /// result.
            pub mod calls {
                #(#calls)*
            }
        }
    })
}

/// `idls/{name}.json` in the crate's directory or the nearest ancestor.
fn find(name: &Ident) -> syn::Result<PathBuf> {
    let root = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| syn::Error::new(name.span(), "declare_program! needs CARGO_MANIFEST_DIR"))?;
    let file = format!("{name}.json");
    PathBuf::from(&root)
        .ancestors()
        .map(|dir| dir.join("idls").join(&file))
        .find(|path| path.is_file())
        .ok_or_else(|| {
            syn::Error::new(
                name.span(),
                format!("no idls/{file} in {root} or above it; write one with `berm idl {name}`"),
            )
        })
}

/// Every titled object in `schema`, by title.
fn collect(schema: &Value, structs: &mut BTreeMap<String, Value>) {
    if let Some(title) = schema["title"].as_str() {
        structs.insert(title.to_owned(), schema.clone());
    }
    for property in schema["properties"].as_object().into_iter().flatten() {
        collect(property.1, structs);
    }
    collect_inner(&schema["items"], structs);
    collect_inner(&schema["additionalProperties"], structs);
}

fn collect_inner(schema: &Value, structs: &mut BTreeMap<String, Value>) {
    if schema.is_object() {
        collect(schema, structs);
    }
}

/// The Rust type a schema describes. `path` prefixes a struct's name.
fn rust_type(schema: &Value, path: &TokenStream) -> TokenStream {
    if let Some(title) = schema["title"].as_str() {
        let ident = format_ident!("{title}");
        return quote!(#path #ident);
    }
    match schema["type"].as_str() {
        Some("string") => quote!(::berm_lang::__private::String),
        Some("boolean") => quote!(bool),
        Some("number") => quote!(f64),
        Some("integer") if schema["minimum"].as_u64() == Some(0) => quote!(u64),
        Some("integer") => quote!(i64),
        Some("array") => {
            let items = rust_type(&schema["items"], path);
            quote!(::berm_lang::__private::Vec<#items>)
        }
        Some("object") if schema["additionalProperties"].is_object() => {
            let values = rust_type(&schema["additionalProperties"], path);
            quote!(::berm_lang::__private::BTreeMap<::berm_lang::__private::String, #values>)
        }
        _ => quote!(::berm_lang::serde_json::Value),
    }
}

/// A Rust identifier for a JSON name, raw when the name is a keyword.
fn field_ident(name: &str) -> Ident {
    syn::parse_str::<Ident>(name)
        .unwrap_or_else(|_| Ident::new_raw(name, proc_macro2::Span::call_site()))
}
