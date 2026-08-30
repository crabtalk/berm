//! Small transforms the window paints with.

use anyhow::{Context, Result};
use berm_index::Entry;
use berm_oci::{Access, Reference, Registry};
use serde_json::{Map, Value};
use std::{fs, path::Path, str::FromStr};

/// How much of a digest reads as an identity without taking the row.
const DIGEST_SHORT: usize = 12;

/// The `sha256:` an index carries is how a registry addresses the bytes, not
/// part of what tells two of them apart.
pub fn short(digest: &str) -> &str {
    let hex = digest.strip_prefix("sha256:").unwrap_or(digest);
    &hex[..DIGEST_SHORT.min(hex.len())]
}

/// A published program as a person reads it: the name its author gave the
/// image, and the version it was pushed under.
pub fn split(entry: &Entry) -> (&str, &str) {
    let key = entry.key();
    let name = key.rsplit('/').next().unwrap_or(key);
    let version = entry.reference[key.len()..].trim_start_matches([':', '@']);
    (name, version)
}

/// Read an image: a file if one is there, a registry reference otherwise. The
/// file wins, so an image sitting in the working directory is never mistaken
/// for something to go and fetch.
///
/// Blocking — the registry client is.
pub fn image(spec: &str) -> Result<Vec<u8>> {
    let path = Path::new(spec);
    if path.exists() {
        return fs::read(path).with_context(|| format!("cannot read {spec}"));
    }

    let reference = Reference::from_str(spec)
        .with_context(|| format!("{spec:?} is neither a file nor a registry reference"))?;
    Registry::open(&reference, Access::Read)?.pull(&reference.reference)
}

/// An argument object shaped like the schema advertises, with every value
/// empty — what to fill in rather than what to look up.
pub fn skeleton(schema: &Value) -> String {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return "{}".to_owned();
    };
    let arguments = properties
        .iter()
        .map(|(name, spec)| (name.clone(), empty(spec)))
        .collect::<Map<_, _>>();
    serde_json::to_string_pretty(&Value::Object(arguments)).unwrap_or_else(|_| "{}".to_owned())
}

fn empty(spec: &Value) -> Value {
    match spec.get("type").and_then(Value::as_str) {
        Some("string") => Value::String(String::new()),
        Some("integer" | "number") => Value::Number(0.into()),
        Some("boolean") => Value::Bool(false),
        Some("array") => Value::Array(Vec::new()),
        Some("object") => Value::Object(Map::new()),
        _ => Value::Null,
    }
}
