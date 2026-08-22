//! `berm inspect` — what a harness claims to be.

use crate::Client;
use anyhow::Result;
use berm_api::{Harness, ToolSpec};
use serde_json::Value;

pub fn run(client: &Client, name: &str) -> Result<()> {
    show(&client.inspect(name)?);
    Ok(())
}

pub fn show(harness: &Harness) {
    println!("{}", harness.name);
    println!("  digest  {}", harness.digest);
    if !harness.usage.is_empty() {
        println!("  usage   {}", harness.usage);
    }

    for tool in &harness.tools {
        println!();
        println!("  {}", tool.name);
        println!("    {}", tool.description);
        for parameter in parameters(tool) {
            println!("    {parameter}");
        }
    }
}

/// Render the tool's JSON Schema as the argument list it describes. A schema
/// printed raw is something an operator has to read past to see the two fields
/// it declares.
fn parameters(tool: &ToolSpec) -> Vec<String> {
    let Some(properties) = tool.parameters.get("properties").and_then(Value::as_object) else {
        return Vec::new();
    };

    let required: Vec<&str> = tool
        .parameters
        .get("required")
        .and_then(Value::as_array)
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let width = properties.keys().map(String::len).max().unwrap_or(0);
    properties
        .iter()
        .map(|(name, schema)| {
            let kind = schema.get("type").and_then(Value::as_str).unwrap_or("any");
            let required = if required.contains(&name.as_str()) {
                " (required)"
            } else {
                ""
            };
            let description = schema
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default();
            format!("{name:<width$}  {kind:<8}{required:<11}  {description}")
        })
        .collect()
}
