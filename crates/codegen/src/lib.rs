//! `#[program]` — turns a module of plain functions into a loadable program.
//!
//! The exports an image needs are ceremony: a description the host reads at
//! registration, and dispatch from a tool index back to a function. None of it
//! is the author's problem, so none of it is in their source.

use proc_macro::TokenStream;
use quote::quote;
use std::collections::BTreeSet;
use syn::{
    Expr, ExprLit, Item, ItemFn, ItemMod, ItemStruct, Lit, LitStr, Token, parse::Parse,
    parse_macro_input,
};

mod declare;
mod schema;
mod syscalls;

/// Declare the syscalls a program may call, emitting a stub for each.
///
/// ```ignore
/// berm_lang::syscalls! {
///     namespace = "berm";
///     mod fs {
///         /// Read a file whole.
///         fn read(path: &str) -> Vec<u8>;
///     }
/// }
/// ```
///
/// [`hosts!`] takes the same declaration and emits what a host registers under.
/// Each side writes it in its own crate, and drift is caught on the first call:
/// a renamed call hashes to a number nothing is registered for.
#[proc_macro]
pub fn syscalls(input: TokenStream) -> TokenStream {
    parse_macro_input!(input as syscalls::Declaration)
        .expand(syscalls::Side::Guest)
        .into()
}

/// Typed calls, types and events for another program, from its IDL.
///
/// ```ignore
/// berm_lang::declare_program!(hn);
///
/// let feed = hn::calls::latest(&hn::types::Latest { page: 0 })?;
/// match hn::events::Event::read(&logged) { … }
/// ```
///
/// Reads `idls/hn.json`, which `berm idl hn` writes, from the crate's
/// directory or the nearest ancestor that has one.
#[proc_macro]
pub fn declare_program(input: TokenStream) -> TokenStream {
    let name = parse_macro_input!(input as syn::Ident);
    declare::expand(&name)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// The other side of [`syscalls!`]: one constructor per name, taking the
/// implementation and returning the `Syscall` that serves it.
#[proc_macro]
pub fn hosts(input: TokenStream) -> TokenStream {
    parse_macro_input!(input as syscalls::Declaration)
        .expand(syscalls::Side::Host)
        .into()
}

/// Default size of the argument and result buffers, in bytes.
///
/// This is paid on every invocation, not once: the buffers live in `.bss`,
/// which the host zeroes each time it instantiates the guest. Measured against
/// the reference guest, 64 KiB costs a few microseconds per call over 16 KiB, and 4 KiB
/// buys nothing back. `buffer = N` overrides it for a program that needs room.
const DEFAULT_BUFFER: usize = 16 * 1024;

/// JSON Schema for a tool that declares no parameters.
const DEFAULT_PARAMETERS: &str = r#"{"type":"object"}"#;

/// Prefix on every tool's exported symbol, so a tool called `init` or
/// `describe` cannot collide with the exports the ABI reserves.
const TOOL_PREFIX: &str = "berm_tool_";

struct Config {
    buffer: usize,
    usage: String,
    /// Programs this one calls by name, as the author declares them.
    programs: Vec<String>,
    /// Syscalls it makes beyond the ABI's own, each beside what it reaches
    /// through that call.
    syscalls: Vec<(String, Vec<String>)>,
    /// Secrets it names in `{secret:NAME}`.
    secrets: Vec<String>,
    /// Kept only to re-emit as an `include_str!`, so editing the file
    /// rebuilds the program.
    usage_file: Option<syn::LitStr>,
}

impl Parse for Config {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut buffer = DEFAULT_BUFFER;
        let mut usage = String::new();
        let mut usage_file = None;
        let mut programs = Vec::new();
        let mut syscalls = Vec::new();
        let mut secrets = Vec::new();

        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            match key.to_string().as_str() {
                "buffer" => {
                    let size: syn::LitInt = input.parse()?;
                    buffer = size.base10_parse()?;
                }
                // A path rather than the text, because usage runs to
                // paragraphs and `include_str!` cannot help here: a proc
                // macro sees the unexpanded call, never the file. So the
                // macro reads it, and the expansion carries an
                // `include_str!` of its own purely so cargo rebuilds when
                // the file changes.
                "usage_file" => {
                    let path: syn::LitStr = input.parse()?;
                    let root = std::env::var("CARGO_MANIFEST_DIR").map_err(|_| {
                        syn::Error::new(key.span(), "usage_file needs CARGO_MANIFEST_DIR")
                    })?;
                    let full = std::path::Path::new(&root).join(path.value());
                    usage = std::fs::read_to_string(&full).map_err(|e| {
                        syn::Error::new(path.span(), format!("{}: {e}", full.display()))
                    })?;
                    usage_file = Some(path);
                }
                // Declared, never inferred: a target the program computes at
                // the call is invisible to anything reading the image, so a
                // scan of this crate would report a list that is quietly short.
                "programs" => programs = strings(input)?,
                "secrets" => secrets = strings(input)?,
                // `{ "berm.get", "berm.http.fetch" = ["example.com"] }`: each
                // call, and what it reaches through that call.
                "syscalls" => {
                    let entries;
                    syn::braced!(entries in input);
                    while !entries.is_empty() {
                        let name: syn::LitStr = entries.parse()?;
                        let reached = match entries.peek(Token![=]) {
                            true => {
                                entries.parse::<Token![=]>()?;
                                strings(&entries)?
                            }
                            false => Vec::new(),
                        };
                        if syscalls.iter().any(|(seen, _)| *seen == name.value()) {
                            return Err(syn::Error::new(
                                name.span(),
                                "syscall declared twice; list everything it reaches in one entry",
                            ));
                        }
                        syscalls.push((name.value(), reached));
                        if !entries.is_empty() {
                            entries.parse::<Token![,]>()?;
                        }
                    }
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unknown argument: {other} (expected `buffer`, `usage_file`, `programs`, `syscalls` or `secrets`)"
                        ),
                    ));
                }
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(Config {
            buffer,
            usage,
            usage_file,
            programs,
            syscalls,
            secrets,
        })
    }
}

/// A bracketed list of string literals.
fn strings(input: syn::parse::ParseStream) -> syn::Result<Vec<String>> {
    let items;
    syn::bracketed!(items in input);
    Ok(items
        .parse_terminated(<syn::LitStr as Parse>::parse, Token![,])?
        .iter()
        .map(syn::LitStr::value)
        .collect())
}

/// One exported tool, as the description will see it.
struct Tool {
    ident: syn::Ident,
    name: String,
    description: String,
    parameters: String,
    /// The struct its arguments are read as, from `#[args(...)]`. The handler
    /// always receives the raw blob; this gives the schema.
    args: Option<syn::Ident>,
    /// The struct its result is written as, from `#[returns(...)]`.
    returns: Option<syn::Ident>,
    /// JSON Schema for its result, from `returns`.
    output: Option<String>,
    /// From `#[query]`: the tool may not write, emit or arm a wake.
    query: bool,
    /// From `#[deprecated]`.
    deprecated: bool,
    /// Programs whose logs this tool takes, each beside the topics it takes.
    logs: Vec<(String, Vec<String>)>,
}

/// One `#[event]` struct: a typed log, emitted under its name.
struct Event {
    ident: syn::Ident,
    description: String,
    /// JSON Schema for its data.
    fields: String,
    /// From `#[deprecated]`.
    deprecated: bool,
}

/// Declare a program from a module of tool functions.
///
/// ```ignore
/// #[program]
/// mod tools {
///     use berm_lang::{Failed, Out};
///
///     /// Echo the argument blob back.
///     pub fn echo(args: &[u8], out: &mut Out) -> Result<(), Failed> {
///         out.write(args);
///         Ok(())
///     }
/// }
/// ```
///
/// Every `pub fn` in the module becomes a tool. Its doc comment is the
/// description the model reads. `#[args(Struct)]` and `#[returns(Struct)]` name
/// structs declared in the module that give the schemas of its arguments and
/// result, and `#[params("…")]` carries an argument schema literal instead.
/// `#[query]` marks it read-only: the host refuses its writes, logs and wakes.
/// `#[deprecated]` on a tool or an event marks it so in the manifest.
/// An `#[event]` struct in the module is a typed log, emitted with
/// `berm_lang::emit!` under the struct's name.
/// `#[logs("hn" = ["item"])]` subscribes it to a program's logs, delivered as
/// its arguments; without `= [...]` it takes every topic.
#[proc_macro_attribute]
pub fn program(args: TokenStream, item: TokenStream) -> TokenStream {
    let config = parse_macro_input!(args as Config);
    let mut module = parse_macro_input!(item as ItemMod);

    let (tools, events) = match collect(&mut module) {
        Ok(collected) => collected,
        Err(error) => return error.to_compile_error().into(),
    };

    let description = describe(&config, &tools, &events);
    let description_len = description.len();
    let description_bytes =
        syn::LitByteStr::new(description.as_bytes(), proc_macro2::Span::call_site());
    let module_ident = &module.ident;
    let buffer = config.buffer;

    // One export per tool, so the host resolves it the way it resolves any
    // other symbol.
    let exports = tools.iter().map(|tool| {
        let ident = &tool.ident;
        let symbol = syn::Ident::new(&format!("{TOOL_PREFIX}{}", tool.name), ident.span());
        let doc = format!("Tool `{}`. {}", tool.name, tool.description);

        quote! {
            #[doc = #doc]
            #[unsafe(no_mangle)]
            pub extern "C" fn #symbol() {
                let arguments = unsafe { &mut *::core::ptr::addr_of_mut!(_CRABTALK_ARGS) };
                let length = ::berm_lang::abi::read_args(arguments);
                if length > _CRABTALK_BUFFER {
                    return ::berm_lang::abi::fail(b"arguments exceeded the input buffer");
                }
                let arguments = &arguments[..length];

                let buffer = unsafe { &mut *::core::ptr::addr_of_mut!(_CRABTALK_OUT) };
                let mut out = ::berm_lang::Out::new(buffer);

                match #module_ident::#ident(arguments, &mut out) {
                    Ok(()) if out.overflowed() => {
                        ::berm_lang::abi::fail(b"result exceeded the output buffer")
                    }
                    Ok(()) => out.finish(),
                    Err(::berm_lang::Failed) => {
                        ::berm_lang::abi::fail(out.written())
                    }
                }
            }
        }
    });

    // The macro read the usage file itself, which cargo cannot see. This
    // makes the dependency visible so editing it rebuilds the program.
    let usage_rebuild = config.usage_file.as_ref().map(|path| {
        quote! {
            const _CRABTALK_USAGE: &str =
                include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #path));
        }
    });

    quote! {
        #module

        #usage_rebuild

        const _CRABTALK_BUFFER: usize = #buffer;
        static mut _CRABTALK_ARGS: [u8; _CRABTALK_BUFFER] = [0; _CRABTALK_BUFFER];
        static mut _CRABTALK_OUT: [u8; _CRABTALK_BUFFER] = [0; _CRABTALK_BUFFER];

        /// What this program is, as a section rather than an export: the host
        /// reads it out of the image without compiling or running anything, so
        /// learning what a program claims never means executing it.
        #[used]
        #[cfg_attr(target_arch = "wasm32", unsafe(link_section = ".berm.abi"))]
        static _CRABTALK_ABI: [u8; #description_len] = *#description_bytes;

        /// A program is a binary, and off a guest target a binary needs a
        /// `main` — which is what lets `cargo test` build one natively.
        #[cfg(not(target_arch = "wasm32"))]
        fn main() {}

        #(#exports)*
    }
    .into()
}

/// Pull the tools and events out of the module, along with the structs that
/// describe their arguments and results, stripping the attributes that only
/// exist to be read here.
fn collect(module: &mut ItemMod) -> syn::Result<(Vec<Tool>, Vec<Event>)> {
    let Some((_, items)) = module.content.as_mut() else {
        return Err(syn::Error::new_spanned(
            &module.ident,
            "#[program] needs a module with a body, not a `mod foo;` declaration",
        ));
    };

    let mut tools = Vec::new();
    for item in items.iter_mut() {
        let Item::Fn(function) = item else { continue };
        if !matches!(function.vis, syn::Visibility::Public(_)) {
            continue;
        }
        tools.push(tool(function)?);
    }

    if tools.is_empty() {
        return Err(syn::Error::new_spanned(
            &module.ident,
            "a program needs at least one `pub fn` to expose as a tool",
        ));
    }

    // A tool's argument and result structs give its schemas, and every struct
    // they reach gets the derive that reads or writes it. The author writes
    // neither.
    let declared: Vec<ItemStruct> = items
        .iter()
        .filter_map(|item| match item {
            Item::Struct(item) => Some(item.clone()),
            _ => None,
        })
        .collect();
    let mut read = BTreeSet::new();
    let mut written = BTreeSet::new();
    let mut events = Vec::new();
    for item in items.iter_mut() {
        let Item::Struct(item) = item else { continue };
        let before = item.attrs.len();
        item.attrs.retain(|a| !a.path().is_ident("event"));
        if item.attrs.len() == before {
            continue;
        }
        let before = item.attrs.len();
        item.attrs.retain(|a| !a.path().is_ident("deprecated"));
        let deprecated = item.attrs.len() != before;
        let (fields, used) = schema::of(&item.ident, &declared)?;
        written.extend(used);
        let description = docs(&item.attrs);
        if description.is_empty() {
            return Err(syn::Error::new_spanned(
                &item.ident,
                "an event needs a doc comment — it is what a reader of the logs is told it means",
            ));
        }
        events.push(Event {
            ident: item.ident.clone(),
            description,
            fields,
            deprecated,
        });
    }
    for tool in &mut tools {
        if let Some(ty) = &tool.args {
            let (schema, used) = schema::of(ty, &declared)?;
            tool.parameters = schema;
            read.extend(used);
        }
        if let Some(ty) = &tool.returns {
            let (schema, used) = schema::of(ty, &declared)?;
            tool.output = Some(schema);
            written.extend(used);
        }
    }

    for item in items.iter_mut() {
        let Item::Struct(item) = item else { continue };
        let name = item.ident.to_string();
        let (reads, writes) = (read.contains(&name), written.contains(&name));
        if reads {
            // An argument struct may be read for its shape alone, never
            // constructed. Saying so beats an author silencing the warning.
            item.attrs.push(syn::parse_quote!(#[allow(dead_code)]));
            item.attrs
                .push(syn::parse_quote!(#[derive(::berm_lang::serde::Deserialize)]));
        }
        if writes {
            item.attrs
                .push(syn::parse_quote!(#[derive(::berm_lang::serde::Serialize)]));
            // The schema leaves an optional field out of `required` rather than
            // allowing `null`, so an absent value is written as an absent key.
            for field in item.fields.iter_mut() {
                if schema::optional(&field.ty) {
                    field.attrs.push(syn::parse_quote!(
                        #[serde(skip_serializing_if = "::core::option::Option::is_none")]
                    ));
                }
            }
        }
        if reads || writes {
            // Reached through the SDK rather than named directly, so a program
            // depends on one crate and cannot pick a serde the derive
            // disagrees with.
            item.attrs
                .push(syn::parse_quote!(#[serde(crate = "::berm_lang::serde")]));
        }
    }

    for event in &events {
        let ident = &event.ident;
        let name = ident.to_string();
        items.push(syn::parse_quote! {
            impl ::berm_lang::Event for #ident {
                const NAME: &'static str = #name;
            }
        });
    }

    Ok((tools, events))
}

fn tool(function: &mut ItemFn) -> syn::Result<Tool> {
    let description = docs(&function.attrs);
    let args = args_of(function)?;
    let mut parameters = None;
    let mut returns = None;
    let mut query = false;
    let deprecated = function
        .attrs
        .iter()
        .any(|a| a.path().is_ident("deprecated"));
    let mut logs: Vec<(String, Vec<String>)> = Vec::new();

    for attribute in &function.attrs {
        if attribute.path().is_ident("params") {
            parameters = Some(attribute.parse_args::<LitStr>()?.value());
        }
        if attribute.path().is_ident("returns") {
            returns = Some(attribute.parse_args::<syn::Ident>()?);
        }
        if attribute.path().is_ident("query") {
            attribute.meta.require_path_only()?;
            query = true;
        }
        if attribute.path().is_ident("logs") {
            let (program, topics) =
                attribute.parse_args_with(|input: syn::parse::ParseStream| {
                    let program: LitStr = input.parse()?;
                    let topics = match input.peek(Token![=]) {
                        true => {
                            input.parse::<Token![=]>()?;
                            strings(input)?
                        }
                        false => Vec::new(),
                    };
                    Ok((program, topics))
                })?;
            if logs.iter().any(|(seen, _)| *seen == program.value()) {
                return Err(syn::Error::new(
                    program.span(),
                    "program subscribed twice; list every topic in one #[logs]",
                ));
            }
            logs.push((program.value(), topics));
        }
    }

    // These exist for this macro to read; leaving them would reach the
    // compiler as unknown attributes.
    function.attrs.retain(|a| {
        !["params", "returns", "query", "logs", "deprecated"]
            .iter()
            .any(|name| a.path().is_ident(name))
    });

    if description.is_empty() {
        return Err(syn::Error::new_spanned(
            &function.sig.ident,
            "a tool needs a doc comment — it is what the model reads to decide whether to call it",
        ));
    }

    Ok(Tool {
        ident: function.sig.ident.clone(),
        name: function.sig.ident.to_string(),
        description,
        args,
        parameters: parameters.unwrap_or_else(|| DEFAULT_PARAMETERS.to_owned()),
        returns,
        output: None,
        query,
        deprecated,
        logs,
    })
}

/// Where a tool's schema comes from: `#[args(Shape)]` names a struct declared
/// beside it. The handler still receives bytes.
fn args_of(function: &mut ItemFn) -> syn::Result<Option<syn::Ident>> {
    let mut declared = None;
    for attribute in &function.attrs {
        if attribute.path().is_ident("args") {
            declared = Some(attribute.parse_args::<syn::Ident>()?);
        }
    }
    function.attrs.retain(|a| !a.path().is_ident("args"));

    match function.sig.inputs.first() {
        Some(syn::FnArg::Typed(first)) if matches!(&*first.ty, syn::Type::Reference(r) if matches!(&*r.elem, syn::Type::Slice(_))) =>
            {}
        _ => {
            return Err(syn::Error::new_spanned(
                &function.sig,
                "a tool takes the argument blob as `&[u8]` and an `&mut Out` to write its result \
                 into — declare its shape with #[args(Struct)] if it has one",
            ));
        }
    }

    Ok(declared)
}

/// Join a doc comment into one line — the description is a JSON string.
pub(crate) fn docs(attrs: &[syn::Attribute]) -> String {
    let mut description = String::new();
    for attribute in attrs {
        if attribute.path().is_ident("doc")
            && let syn::Meta::NameValue(pair) = &attribute.meta
            && let Expr::Lit(ExprLit {
                lit: Lit::Str(line),
                ..
            }) = &pair.value
        {
            if !description.is_empty() {
                description.push(' ');
            }
            description.push_str(line.value().trim());
        }
    }
    description.trim().to_owned()
}

/// Build the description JSON at compile time.
fn describe(config: &Config, tools: &[Tool], events: &[Event]) -> String {
    let specs = tools
        .iter()
        .map(|t| {
            let returns = t
                .output
                .as_ref()
                .map(|schema| format!(r#","returns":{schema}"#))
                .unwrap_or_default();
            let query = match t.query {
                true => r#","query":true"#,
                false => "",
            };
            let deprecated = match t.deprecated {
                true => r#","deprecated":true"#,
                false => "",
            };
            format!(
                r#"{{"name":"{}","description":"{}","parameters":{}{returns}{query}{deprecated}}}"#,
                escape(&t.name),
                escape(&t.description),
                t.parameters,
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    let list = |items: &[String]| {
        items
            .iter()
            .map(|item| format!(r#""{}""#, escape(item)))
            .collect::<Vec<_>>()
            .join(",")
    };
    let programs = list(&config.programs);
    let secrets = list(&config.secrets);
    let syscalls = config
        .syscalls
        .iter()
        .map(|(name, reached)| format!(r#""{}":[{}]"#, escape(name), list(reached)))
        .collect::<Vec<_>>()
        .join(",");
    let logs = tools
        .iter()
        .flat_map(|tool| {
            tool.logs.iter().map(|(program, topics)| {
                format!(
                    r#"{{"program":"{}","topics":[{}],"tool":"{}"}}"#,
                    escape(program),
                    list(topics),
                    escape(&tool.name)
                )
            })
        })
        .collect::<Vec<_>>()
        .join(",");

    let events = events
        .iter()
        .map(|event| {
            let deprecated = match event.deprecated {
                true => r#","deprecated":true"#,
                false => "",
            };
            format!(
                r#"{{"name":"{}","description":"{}","fields":{}{deprecated}}}"#,
                event.ident,
                escape(&event.description),
                event.fields
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    format!(
        r#"{{"abi_version":3,"version":"{}","tools":[{specs}],"usage":"{}","deps":{{"programs":[{programs}],"syscalls":{{{syscalls}}},"secrets":[{secrets}],"logs":[{logs}]}},"events":[{events}]}}"#,
        escape(&std::env::var("CARGO_PKG_VERSION").unwrap_or_default()),
        escape(&config.usage),
    )
}

/// Minimal JSON string escaping — doc comments are prose and can carry
/// anything, and a stray quote would produce a description the host cannot
/// parse at all.
pub(crate) fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if (c as u32) < 0x20 => escaped.push_str(&format!("\\u{:04x}", c as u32)),
            c => escaped.push(c),
        }
    }
    escaped
}
