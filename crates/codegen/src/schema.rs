//! Rust types to JSON Schema, at compile time.
//!
//! A tool's arguments and result are structs the author already wrote; the
//! schema is derived from them rather than repeated by hand in a string literal
//! that nothing checks. Field doc comments become descriptions, `Option<T>` is
//! what makes a field optional, and a field may name another struct declared in
//! the same module, which is inlined.

use crate::docs;
use syn::{Fields, GenericArgument, Ident, ItemStruct, PathArguments, Type};

/// The schema of the struct `name` declared in the module, and the name of
/// every struct it inlines, itself included.
pub fn of(name: &Ident, declared: &[ItemStruct]) -> syn::Result<(String, Vec<String>)> {
    let Some(item) = declared.iter().find(|item| item.ident == *name) else {
        return Err(syn::Error::new_spanned(
            name,
            format!(
                "`{name}` is not declared in this module — a tool's argument and result structs \
                 have to live beside it so the schema can be derived from their fields"
            ),
        ));
    };
    let mut walk = Walk {
        declared,
        used: Vec::new(),
        open: Vec::new(),
    };
    let schema = walk.object(item)?;
    Ok((schema, walk.used))
}

/// Whether a field may be omitted: `Option<T>`.
pub fn optional(ty: &Type) -> bool {
    unwrap_option(ty).1
}

struct Walk<'a> {
    declared: &'a [ItemStruct],
    /// Every struct inlined so far.
    used: Vec<String>,
    /// The structs being inlined, outermost first. A struct that reaches itself
    /// has no finite schema.
    open: Vec<String>,
}

impl Walk<'_> {
    fn object(&mut self, item: &ItemStruct) -> syn::Result<String> {
        let Fields::Named(fields) = &item.fields else {
            return Err(syn::Error::new_spanned(
                &item.ident,
                "a tool's arguments and result must be structs with named fields — the names are \
                 the JSON object's keys",
            ));
        };
        let name = item.ident.to_string();
        self.open.push(name.clone());
        if !self.used.contains(&name) {
            self.used.push(name);
        }

        let mut properties = Vec::new();
        let mut required = Vec::new();

        for field in &fields.named {
            let name = field
                .ident
                .as_ref()
                .expect("named fields have idents")
                .to_string();
            let (ty, optional) = unwrap_option(&field.ty);
            let description = docs(&field.attrs);

            let mut property = self.type_of(ty)?;
            if !description.is_empty() {
                // Exactly one brace: a nested type like `Vec<T>` or a map ends
                // in two, and trimming both puts the description inside the
                // inner object and leaves the JSON one brace short.
                let open = property
                    .strip_suffix('}')
                    .ok_or_else(|| unsupported(&field.ty))?;
                property = format!(
                    r#"{open},"description":"{}"}}"#,
                    crate::escape(&description)
                );
            }

            properties.push(format!(r#""{}":{}"#, crate::escape(&name), property));
            if !optional {
                required.push(format!(r#""{}""#, crate::escape(&name)));
            }
        }

        self.open.pop();
        let properties = properties.join(",");
        let required = required.join(",");
        // `title` names the struct, which is what `declare_program!` names the
        // type it generates after.
        Ok(format!(
            r#"{{"type":"object","title":"{}","properties":{{{properties}}},"required":[{required}]}}"#,
            item.ident
        ))
    }

    /// The JSON Schema fragment for one Rust type.
    fn type_of(&mut self, ty: &Type) -> syn::Result<String> {
        if let Type::Reference(reference) = ty {
            return self.type_of(&reference.elem);
        }

        let Type::Path(path) = ty else {
            return Err(unsupported(ty));
        };
        let Some(segment) = path.path.segments.last() else {
            return Err(unsupported(ty));
        };

        let name = segment.ident.to_string();
        if name == "Vec" {
            let PathArguments::AngleBracketed(args) = &segment.arguments else {
                return Err(unsupported(ty));
            };
            let Some(GenericArgument::Type(inner)) = args.args.first() else {
                return Err(unsupported(ty));
            };
            return Ok(format!(
                r#"{{"type":"array","items":{}}}"#,
                self.type_of(inner)?
            ));
        }

        // A map is an open object: the keys are the caller's, so only the
        // values have a schema. Keys are strings because that is what JSON
        // objects have.
        if name == "BTreeMap" || name == "HashMap" {
            let PathArguments::AngleBracketed(args) = &segment.arguments else {
                return Err(unsupported(ty));
            };
            let mut types = args.args.iter().filter_map(|arg| match arg {
                GenericArgument::Type(ty) => Some(ty),
                _ => None,
            });
            let (Some(key), Some(value)) = (types.next(), types.next()) else {
                return Err(unsupported(ty));
            };
            if self.type_of(key)? != r#"{"type":"string"}"# {
                return Err(syn::Error::new_spanned(
                    key,
                    "a map's keys become JSON object keys, so they have to be strings",
                ));
            }
            return Ok(format!(
                r#"{{"type":"object","additionalProperties":{}}}"#,
                self.type_of(value)?
            ));
        }

        let declared = self.declared;
        if path.path.segments.len() == 1
            && let Some(item) = declared.iter().find(|item| item.ident == segment.ident)
        {
            if self.open.contains(&name) {
                return Err(syn::Error::new_spanned(
                    ty,
                    format!("`{name}` contains itself, which no JSON Schema here can describe"),
                ));
            }
            return self.object(item);
        }

        Ok(match name.as_str() {
            "String" | "str" => r#"{"type":"string"}"#,
            "bool" => r#"{"type":"boolean"}"#,
            "f32" | "f64" => r#"{"type":"number"}"#,
            "u8" | "u16" | "u32" | "u64" | "usize" => r#"{"type":"integer","minimum":0}"#,
            "i8" | "i16" | "i32" | "i64" | "isize" => r#"{"type":"integer"}"#,
            _ => return Err(unsupported(ty)),
        }
        .to_owned())
    }
}

/// `Option<T>` means the field may be omitted; everything else is required.
fn unwrap_option(ty: &Type) -> (&Type, bool) {
    if let Type::Path(path) = ty
        && let Some(segment) = path.path.segments.last()
        && segment.ident == "Option"
        && let PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(GenericArgument::Type(inner)) = args.args.first()
    {
        return (inner, true);
    }
    (ty, false)
}

fn unsupported(ty: &Type) -> syn::Error {
    syn::Error::new_spanned(
        ty,
        "a field must be a string, number, boolean, Vec, string-keyed map, Option of those, or a \
         struct declared in the program's module — there is no JSON Schema for this type",
    )
}
