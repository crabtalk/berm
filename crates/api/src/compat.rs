//! Whether a program's next manifest keeps working for the callers of this one.
//!
//! Arguments are read by the new program from callers built against the old
//! one; results and events are read by those callers from the new program.
//! Fields a reader does not know are ignored.

use crate::{EventSpec, Manifest, ToolSpec};
use serde_json::Value;

/// One way the next manifest breaks a caller of this one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Break {
    pub surface: Surface,
    pub reason: String,
}

/// What a [`Break`] is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Surface {
    Tool(String),
    Event(String),
}

impl std::fmt::Display for Break {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.surface {
            Surface::Tool(name) => write!(f, "tool {name}: {}", self.reason),
            Surface::Event(name) => write!(f, "event {name}: {}", self.reason),
        }
    }
}

impl Manifest {
    /// The tools `next` makes public that this manifest does not.
    pub fn publishes<'a>(&self, next: &'a Manifest) -> Vec<&'a str> {
        next.tools
            .iter()
            .filter(|tool| tool.public)
            .filter(|tool| {
                !self
                    .tools
                    .iter()
                    .any(|old| old.public && old.name == tool.name)
            })
            .map(|tool| tool.name.as_str())
            .collect()
    }

    /// Every way `next` breaks a caller of this manifest: its public tools,
    /// and its events. Empty when it is compatible.
    pub fn breaks(&self, next: &Manifest) -> Vec<Break> {
        let mut breaks = Vec::new();
        for old in self.tools.iter().filter(|tool| tool.public) {
            let surface = Surface::Tool(old.name.clone());
            let mut found = Vec::new();
            match next.tools.iter().find(|tool| tool.name == old.name) {
                None => found.push("removed".to_owned()),
                Some(new) => tool(old, new, &mut found),
            }
            breaks.extend(found.into_iter().map(|reason| Break {
                surface: surface.clone(),
                reason,
            }));
        }
        for old in &self.events {
            let surface = Surface::Event(old.name.clone());
            let mut found = Vec::new();
            match next.events.iter().find(|event| event.name == old.name) {
                None => found.push("removed".to_owned()),
                Some(new) => event(old, new, &mut found),
            }
            breaks.extend(found.into_iter().map(|reason| Break {
                surface: surface.clone(),
                reason,
            }));
        }
        breaks
    }
}

fn tool(old: &ToolSpec, new: &ToolSpec, found: &mut Vec<String>) {
    if !new.public {
        found.push("no longer public".to_owned());
        return;
    }
    if new.callers.is_some() && old.callers != new.callers {
        found.push("takes fewer kinds of caller".to_owned());
    }
    if old.query && !new.query {
        found.push("no longer a query".to_owned());
    }
    accepts("arguments", &old.parameters, &new.parameters, found);
    match (&old.returns, &new.returns) {
        (Some(old), Some(new)) => writes("result", old, new, found),
        (Some(_), None) => found.push("result no longer declared".to_owned()),
        (None, _) => {}
    }
}

fn event(old: &EventSpec, new: &EventSpec, found: &mut Vec<String>) {
    writes("data", &old.fields, &new.fields, found);
}

/// Whether what `new` reads still takes everything callers wrote against `old`.
fn accepts(at: &str, old: &Value, new: &Value, found: &mut Vec<String>) {
    if !same_type(at, old, new, found) {
        return;
    }
    if old.get("minimum").is_none() && new.get("minimum").is_some() {
        found.push(format!("{at} no longer takes a negative integer"));
    }
    let old_required = required(old);
    for name in required(new) {
        if !old_required.contains(&name) {
            found.push(format!("{at}.{name} is newly required"));
        }
    }
    each_shared(at, old, new, found, accepts);
}

/// Whether what `new` writes is still read by callers built against `old`.
fn writes(at: &str, old: &Value, new: &Value, found: &mut Vec<String>) {
    if !same_type(at, old, new, found) {
        return;
    }
    if old.get("minimum").is_some() && new.get("minimum").is_none() {
        found.push(format!("{at} may now be a negative integer"));
    }
    let new_required = required(new);
    for name in required(old) {
        if !new_required.contains(&name) {
            found.push(format!("{at}.{name} is no longer always present"));
        }
    }
    each_shared(at, old, new, found, writes);
}

/// `false`, having said so, when the two do not have the same JSON type.
fn same_type(at: &str, old: &Value, new: &Value, found: &mut Vec<String>) -> bool {
    let (old_type, new_type) = (old.get("type"), new.get("type"));
    if old_type == new_type {
        return true;
    }
    let name = |ty: Option<&Value>| ty.and_then(Value::as_str).unwrap_or("any").to_owned();
    found.push(format!(
        "{at} changed from {} to {}",
        name(old_type),
        name(new_type)
    ));
    false
}

/// Compare what the two have in common: properties by name, array items and
/// map values.
fn each_shared(
    at: &str,
    old: &Value,
    new: &Value,
    found: &mut Vec<String>,
    compare: fn(&str, &Value, &Value, &mut Vec<String>),
) {
    if let (Some(old), Some(new)) = (
        old.get("properties").and_then(Value::as_object),
        new.get("properties").and_then(Value::as_object),
    ) {
        for (name, old) in old {
            if let Some(new) = new.get(name) {
                compare(&format!("{at}.{name}"), old, new, found);
            }
        }
    }
    for key in ["items", "additionalProperties"] {
        if let (Some(old), Some(new)) = (old.get(key), new.get(key))
            && old.is_object()
            && new.is_object()
        {
            compare(&format!("{at}[]"), old, new, found);
        }
    }
}

fn required(schema: &Value) -> Vec<String> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
