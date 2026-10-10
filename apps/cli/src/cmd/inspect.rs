//! `berm inspect` — what a program claims to be.

use crate::Client;
use anyhow::Result;
use berm_api::Program;
use serde_json::Value;

pub fn run(client: &Client, name: &str) -> Result<()> {
    show(&client.inspect(name)?);
    Ok(())
}

pub fn show(program: &Program) {
    println!("{}", program.name);
    if !program.version.is_empty() {
        println!("  version {}", program.version);
    }
    println!("  digest  {}", program.digest);
    if !program.usage.is_empty() {
        println!("  usage   {}", program.usage);
    }
    let answered = |missing: bool| match missing {
        true => "  (nothing answers to it here)",
        false => "",
    };
    for dep in &program.deps.programs {
        let missing = program.unresolved.programs.contains(dep);
        println!("  program {dep}{}", answered(missing));
    }
    for (name, reached) in &program.deps.syscalls {
        let missing = program.unresolved.syscalls.contains_key(name);
        let reached = match reached.is_empty() {
            true => String::new(),
            false => format!(" -> {}", reached.join(", ")),
        };
        println!("  syscall {name}{reached}{}", answered(missing));
    }
    for name in &program.deps.secrets {
        let missing = program.unresolved.secrets.contains(name);
        println!("  secret  {name}{}", answered(missing));
    }
    for broken in &program.breaks {
        println!("  breaks  {broken}");
    }

    for tool in &program.tools {
        println!();
        let marks = [(tool.query, " (query)"), (tool.deprecated, " (deprecated)")];
        let marks: String = marks
            .iter()
            .filter(|(set, _)| *set)
            .map(|(_, mark)| *mark)
            .collect();
        println!("  {}{marks}", tool.name);
        println!("    {}", tool.description);
        for parameter in properties(&tool.parameters) {
            println!("    {parameter}");
        }
        if let Some(returns) = &tool.returns {
            println!("    returns");
            for field in properties(returns) {
                println!("      {field}");
            }
        }
    }

    for event in &program.events {
        println!();
        match event.deprecated {
            true => println!("  event {} (deprecated)", event.name),
            false => println!("  event {}", event.name),
        }
        println!("    {}", event.description);
        for field in properties(&event.fields) {
            println!("    {field}");
        }
    }
}

/// Render an object's JSON Schema as the field list it describes. A schema
/// printed raw is something an operator has to read past to see the two fields
/// it declares.
fn properties(schema: &Value) -> Vec<String> {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return Vec::new();
    };

    let required: Vec<&str> = schema
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
