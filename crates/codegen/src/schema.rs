//! `#[derive(Schema)]`: a type's JSON Schema, and the serde impls that read
//! and write it as that schema says.

use crate::{docs, escape};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, GenericArgument, PathArguments, Type, ext::IdentExt};

pub fn derive(input: &DeriveInput) -> syn::Result<TokenStream> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "a Schema type takes no generics — its schema is one fixed shape",
        ));
    }
    let ident = &input.ident;
    let title = ident.to_string();

    let (shape, remote) = match &input.data {
        Data::Struct(data) => {
            let Fields::Named(fields) = &data.fields else {
                return Err(syn::Error::new_spanned(
                    ident,
                    "a Schema struct has named fields — the names are the JSON object's keys",
                ));
            };
            let shapes = fields.named.iter().map(|field| {
                let name = field.ident.as_ref().expect("named").unraw().to_string();
                let description = escape(&docs(&field.attrs));
                let ty = &field.ty;
                quote! {
                    ::berm_lang::schema::Field {
                        name: #name,
                        description: #description,
                        shape: &<#ty as ::berm_lang::Schema>::SHAPE,
                    }
                }
            });
            let mirrored = fields.named.iter().map(|field| {
                let name = &field.ident;
                let ty = &field.ty;
                // The schema leaves an optional field out of `required` rather
                // than allowing `null`, so an absent value is an absent key.
                let skip = optional(ty).then(|| {
                    quote!(#[serde(default, skip_serializing_if = "::core::option::Option::is_none")])
                });
                quote!(#skip #name: #ty,)
            });
            (
                quote! {
                    ::berm_lang::schema::Shape::Object {
                        title: #title,
                        fields: &[#(#shapes),*],
                    }
                },
                quote!(struct Remote { #(#mirrored)* }),
            )
        }
        Data::Enum(data) => {
            let mut values = Vec::new();
            for variant in &data.variants {
                if !matches!(variant.fields, Fields::Unit) {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "a Schema enum's variants carry nothing — it is a string that is one of them",
                    ));
                }
                values.push(snake(&variant.ident.to_string()));
            }
            let variants = data.variants.iter().map(|variant| &variant.ident);
            (
                quote! {
                    ::berm_lang::schema::Shape::Enum {
                        title: #title,
                        values: &[#(#values),*],
                    }
                },
                quote! {
                    #[serde(rename_all = "snake_case")]
                    enum Remote { #(#variants),* }
                },
            )
        }
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(ident, "a union has no JSON Schema"));
        }
    };

    let remote_path = title.as_str();

    Ok(quote! {
        impl ::berm_lang::Schema for #ident {
            const SHAPE: ::berm_lang::schema::Shape = #shape;
        }

        // serde's remote derive, so the type itself carries no serde
        // attributes and the author writes one derive.
        const _: () = {
            #[derive(::berm_lang::serde::Serialize, ::berm_lang::serde::Deserialize)]
            #[serde(crate = "::berm_lang::serde", remote = #remote_path)]
            #remote

            impl ::berm_lang::serde::Serialize for #ident {
                fn serialize<S: ::berm_lang::serde::Serializer>(
                    &self,
                    serializer: S,
                ) -> ::core::result::Result<S::Ok, S::Error> {
                    Remote::serialize(self, serializer)
                }
            }

            impl<'de> ::berm_lang::serde::Deserialize<'de> for #ident {
                fn deserialize<D: ::berm_lang::serde::Deserializer<'de>>(
                    deserializer: D,
                ) -> ::core::result::Result<Self, D::Error> {
                    Remote::deserialize(deserializer)
                }
            }
        };
    })
}

/// `#[event]`: a `Schema` struct emitted as a log under its name, and its
/// record in the manifest.
pub fn event(mut item: syn::ItemStruct) -> syn::Result<TokenStream> {
    let before = item.attrs.len();
    item.attrs.retain(|a| !a.path().is_ident("deprecated"));
    let deprecated = match item.attrs.len() != before {
        true => r#","deprecated":true"#,
        false => "",
    };
    let description = docs(&item.attrs);
    if description.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.ident,
            "an event needs a doc comment — it is what a reader of the logs is told it means",
        ));
    }
    let ident = &item.ident;
    let name = ident.to_string();
    let head = format!(
        r#"{{"event":{{"name":"{name}","description":"{}","fields":"#,
        escape(&description)
    );
    let tail = format!("{deprecated}}}}}");

    Ok(quote! {
        #[derive(::berm_lang::Schema)]
        #item

        impl ::berm_lang::Event for #ident {
            const NAME: &'static str = #name;
        }

        // One record of the manifest; the linker joins every record in the
        // section.
        const _: () = {
            const PIECES: &[::berm_lang::schema::Piece] = &[
                ::berm_lang::schema::Piece::Text(#head),
                ::berm_lang::schema::Piece::Shape(&<#ident as ::berm_lang::Schema>::SHAPE),
                ::berm_lang::schema::Piece::Text(#tail),
            ];
            #[used]
            #[cfg_attr(target_arch = "wasm32", unsafe(link_section = ".berm.abi"))]
            static RECORD: [u8; ::berm_lang::schema::len(PIECES)] =
                ::berm_lang::schema::render(PIECES);
        };
    })
}

/// `#[error_code]`: an enum of failures, each failing a tool with its
/// `#[msg("…")]`.
pub fn error_code(mut item: syn::ItemEnum) -> syn::Result<TokenStream> {
    let mut arms = Vec::new();
    for variant in &mut item.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(syn::Error::new_spanned(
                &variant.fields,
                "an error code carries nothing but its message",
            ));
        }
        let Some(at) = variant.attrs.iter().position(|a| a.path().is_ident("msg")) else {
            return Err(syn::Error::new_spanned(
                &variant.ident,
                "an error code needs #[msg(\"…\")] — it is what the caller reads",
            ));
        };
        let message: syn::LitStr = variant.attrs.remove(at).parse_args()?;
        let ident = &variant.ident;
        arms.push(quote!(Self::#ident => #message));
    }
    let ident = &item.ident;
    Ok(quote! {
        #item

        impl #ident {
            /// What a tool failing with it says.
            pub fn message(&self) -> &'static str {
                match self {
                    #(#arms,)*
                }
            }
        }

        impl ::core::convert::From<#ident> for ::berm_lang::Error {
            fn from(error: #ident) -> Self {
                error.message().into()
            }
        }
    })
}

/// Whether a type is `Option<T>`, which may be absent.
pub fn optional(ty: &Type) -> bool {
    if let Type::Path(path) = ty
        && let Some(segment) = path.path.segments.last()
        && segment.ident == "Option"
        && let PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(GenericArgument::Type(_)) = args.args.first()
    {
        return true;
    }
    false
}

/// A variant's name as serde's `snake_case` writes it.
fn snake(name: &str) -> String {
    let mut snake = String::new();
    for (at, character) in name.char_indices() {
        if at > 0 && character.is_uppercase() {
            snake.push('_');
        }
        snake.push(character.to_ascii_lowercase());
    }
    snake
}
