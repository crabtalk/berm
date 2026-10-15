//! `#[program]` — turns a module of plain functions into a loadable program.
//!
//! The exports an image needs are ceremony: a description the host reads at
//! registration, and dispatch from a tool index back to a function. None of it
//! is the author's problem, so none of it is in their source.
//!
//! `#[derive(Schema)]`, `#[event]` and `#[error_code]` go on types anywhere in
//! the program's crate.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    DeriveInput, Expr, ExprLit, FnArg, GenericArgument, Item, ItemFn, ItemMod, Lit, Pat,
    PathArguments, ReturnType, Token, Type, ext::IdentExt, parse::Parse, parse_macro_input,
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
/// let lanes = hn::calls::lanes()?;
/// match hn::events::Event::read(&logged) { … }
/// ```
///
/// Reads `idls/hn.json`, which `crab idl hn` writes, from the crate's
/// directory or the nearest ancestor that has one. A call takes the tool's
/// arguments in the IDL's key order, an optional one as an `Option`.
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

/// Default size of the argument buffer, and the most a result may be, in
/// bytes.
///
/// The buffer is paid on every invocation, not once: it lives in `.bss`, which
/// the host zeroes each time it instantiates the guest. Measured against the
/// reference guest, 64 KiB costs a few microseconds per call over 16 KiB, and
/// 4 KiB buys nothing back. `buffer = N` overrides it for a program that needs
/// room.
const DEFAULT_BUFFER: usize = 16 * 1024;

/// JSON Schema for a tool that takes no parameters.
const DEFAULT_PARAMETERS: &str = r#"{"type":"object"}"#;

/// Prefix on every tool's exported symbol, so a tool called `init` or
/// `describe` cannot collide with the exports the ABI reserves.
const TOOL_PREFIX: &str = "berm_tool_";

/// Prefix on the function in the program's module that reads a tool's
/// arguments, calls it and writes its reply.
const DISPATCH_PREFIX: &str = "__berm_";

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
    /// The database's migrations directory, relative to the crate, and each
    /// `*.sql` file in it by name.
    database: Option<(syn::LitStr, Vec<(String, String)>)>,
}

impl Parse for Config {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut buffer = DEFAULT_BUFFER;
        let mut usage = String::new();
        let mut usage_file = None;
        let mut programs = Vec::new();
        let mut syscalls = Vec::new();
        let mut secrets = Vec::new();
        let mut database = None;

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
                // A directory of `*.sql` migrations, applied in name order.
                // Read here for the same reason `usage_file` is.
                "database" => {
                    let path: syn::LitStr = input.parse()?;
                    let root = std::env::var("CARGO_MANIFEST_DIR").map_err(|_| {
                        syn::Error::new(key.span(), "database needs CARGO_MANIFEST_DIR")
                    })?;
                    let dir = std::path::Path::new(&root).join(path.value());
                    let unreadable = |e: std::io::Error| {
                        syn::Error::new(path.span(), format!("{}: {e}", dir.display()))
                    };
                    let mut migrations = Vec::new();
                    for entry in std::fs::read_dir(&dir).map_err(unreadable)? {
                        let file = entry.map_err(unreadable)?.path();
                        if file.extension().is_none_or(|extension| extension != "sql") {
                            continue;
                        }
                        let name = file
                            .file_name()
                            .and_then(|name| name.to_str())
                            .ok_or_else(|| {
                                syn::Error::new(path.span(), "a migration name is not UTF-8")
                            })?
                            .to_owned();
                        let sql = std::fs::read_to_string(&file).map_err(unreadable)?;
                        migrations.push((name, sql));
                    }
                    migrations.sort();
                    database = Some((path, migrations));
                }
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
                            "unknown argument: {other} (expected `buffer`, `usage_file`, `database`, `programs`, `syscalls` or `secrets`)"
                        ),
                    ));
                }
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        // A program keeping a database calls `berm.sql`, and says so like any
        // other syscall.
        if database.is_some() && !syscalls.iter().any(|(name, _)| name == "berm.sql") {
            syscalls.push(("berm.sql".to_owned(), Vec::new()));
        }

        Ok(Config {
            buffer,
            usage,
            usage_file,
            database,
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
    params: Vec<Param>,
    /// The `T` of its `Result<T>`; `None` for `()`, which replies nothing.
    reply: Option<Type>,
    /// From `#[query]`: the tool may not write, emit or arm a wake.
    query: bool,
    /// From `#[deprecated]`.
    deprecated: bool,
    /// From `#[public]`: any caller may reach it.
    public: bool,
    /// From `#[callers(services)]` or `#[callers(accounts)]`.
    callers: Option<String>,
}

/// One parameter: a property of the tool's arguments.
struct Param {
    ident: syn::Ident,
    ty: Type,
    description: String,
}

/// Declare a program from a module of tool functions.
///
/// ```ignore
/// #[program]
/// mod tools {
///     use berm_lang::Result;
///
///     /// Greets `name`.
///     #[public]
///     pub fn greet(
///         /// Who to greet.
///         name: String,
///     ) -> Result<String> {
///         Ok(format!("hello, {name}"))
///     }
/// }
/// ```
///
/// Every `pub fn` in the module becomes a tool. Its doc comment is the
/// description the model reads. Its parameters are the properties of its
/// arguments, each a `Schema` type, and a parameter's doc comment is its
/// description. It returns `berm_lang::Result<T>`: `T` is its reply, written as
/// JSON, and `()` replies nothing.
///
/// **A tool is internal unless marked `#[public]`.** An internal tool is only
/// called by the host, for the program's own wakes. A
/// `#[public]` tool can be called by any account the host authenticates and
/// any program allowed to reach this one, with arguments the caller chooses.
/// `berm_lang::caller()` says who called. `#[callers(services)]` limits a
/// public tool to programs, `#[callers(accounts)]` to callers from outside.
/// `#[query]` marks it read-only: the host refuses its writes, logs and wakes.
/// `#[deprecated]` on a tool marks it so in the manifest.
#[proc_macro_attribute]
pub fn program(args: TokenStream, item: TokenStream) -> TokenStream {
    let config = parse_macro_input!(args as Config);
    let mut module = parse_macro_input!(item as ItemMod);

    let tools = match collect(&mut module) {
        Ok(tools) => tools,
        Err(error) => return error.to_compile_error().into(),
    };

    let pieces = manifest(&config, &tools);
    let module_ident = &module.ident;
    let buffer = config.buffer;

    let (_, items) = module.content.as_mut().expect("collect checked the body");
    for tool in &tools {
        items.push(dispatch(tool));
    }
    // In the module, where the types its shapes name resolve.
    items.push(syn::parse_quote! {
        // What this program is, as a section rather than an export: the host
        // reads it out of the image without compiling or running anything, so
        // learning what a program claims never means executing it.
        const _: () = {
            const PIECES: &[::berm_lang::schema::Piece] = &[#(#pieces),*];
            #[used]
            #[cfg_attr(target_arch = "wasm32", unsafe(link_section = ".berm.abi"))]
            static ABI: [u8; ::berm_lang::schema::len(PIECES)] =
                ::berm_lang::schema::render(PIECES);
        };
    });

    // One export per tool, so the host resolves it the way it resolves any
    // other symbol.
    let exports = tools.iter().map(|tool| {
        let symbol = format_ident!("{TOOL_PREFIX}{}", tool.name);
        let dispatch = format_ident!("{DISPATCH_PREFIX}{}", tool.name);
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
                match #module_ident::#dispatch(&arguments[..length]) {
                    Ok(reply) if reply.len() > _CRABTALK_BUFFER => {
                        ::berm_lang::abi::fail(b"result exceeded the output buffer")
                    }
                    Ok(reply) => ::berm_lang::abi::done(&reply),
                    Err(error) => ::berm_lang::abi::fail(error.message.as_bytes()),
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

    // Each migration was read by the macro too.
    let database_rebuild = config.database.iter().flat_map(|(dir, migrations)| {
        migrations.iter().map(move |(name, _)| {
            let path = format!("{}/{name}", dir.value());
            quote! {
                const _: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #path));
            }
        })
    });

    quote! {
        #module

        #usage_rebuild

        #(#database_rebuild)*

        const _CRABTALK_BUFFER: usize = #buffer;
        static mut _CRABTALK_ARGS: [u8; _CRABTALK_BUFFER] = [0; _CRABTALK_BUFFER];

        /// A program is a binary, and off a guest target a binary needs a
        /// `main` — which is what lets `cargo test` build one natively.
        #[cfg(not(target_arch = "wasm32"))]
        fn main() {}

        #(#exports)*
    }
    .into()
}

/// A type's JSON Schema, and the serde impls that read and write it.
///
/// A struct with named fields is an object titled by its name: a field's doc
/// comment is its description, and an `Option` field may be absent. An enum
/// of unit variants is a string, each variant in `snake_case`.
#[proc_macro_derive(Schema)]
pub fn derive_schema(input: TokenStream) -> TokenStream {
    schema::derive(&parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// A typed log, emitted with `berm_lang::emit!` under the struct's name and
/// listed in the program's manifest. Its doc comment is what a reader of the
/// logs is told it means; `#[deprecated]` marks it so in the manifest.
#[proc_macro_attribute]
pub fn event(args: TokenStream, item: TokenStream) -> TokenStream {
    let _ = parse_macro_input!(args as syn::parse::Nothing);
    schema::event(parse_macro_input!(item as syn::ItemStruct))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// An enum of a program's failures. Each variant takes `#[msg("…")]`, the
/// message a tool failing with it answers, and converts into
/// `berm_lang::Error`.
#[proc_macro_attribute]
pub fn error_code(args: TokenStream, item: TokenStream) -> TokenStream {
    let _ = parse_macro_input!(args as syn::parse::Nothing);
    schema::error_code(parse_macro_input!(item as syn::ItemEnum))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Pull the tools out of the module, stripping the attributes that only exist
/// to be read here.
fn collect(module: &mut ItemMod) -> syn::Result<Vec<Tool>> {
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
    Ok(tools)
}

fn tool(function: &mut ItemFn) -> syn::Result<Tool> {
    let description = docs(&function.attrs);
    let mut query = false;
    let mut public = false;
    let mut callers = None;
    let deprecated = function
        .attrs
        .iter()
        .any(|a| a.path().is_ident("deprecated"));

    for attribute in &function.attrs {
        if attribute.path().is_ident("query") {
            attribute.meta.require_path_only()?;
            query = true;
        }
        if attribute.path().is_ident("public") {
            attribute.meta.require_path_only()?;
            public = true;
        }
        if attribute.path().is_ident("callers") {
            let kind: syn::Ident = attribute.parse_args()?;
            if kind != "services" && kind != "accounts" {
                return Err(syn::Error::new(
                    kind.span(),
                    "expected `services` or `accounts`",
                ));
            }
            callers = Some(kind.to_string());
        }
    }

    // These exist for this macro to read; leaving them would reach the
    // compiler as unknown attributes.
    function.attrs.retain(|a| {
        !["query", "public", "callers", "deprecated"]
            .iter()
            .any(|name| a.path().is_ident(name))
    });

    if description.is_empty() {
        return Err(syn::Error::new_spanned(
            &function.sig.ident,
            "a tool needs a doc comment — it is what the model reads to decide whether to call it",
        ));
    }

    let mut params = Vec::new();
    for input in function.sig.inputs.iter_mut() {
        let FnArg::Typed(typed) = input else {
            return Err(syn::Error::new_spanned(input, "a tool is a free function"));
        };
        let Pat::Ident(pattern) = &*typed.pat else {
            return Err(syn::Error::new_spanned(
                &typed.pat,
                "a tool's parameter is a plain name — it is the argument's key",
            ));
        };
        let description = docs(&typed.attrs);
        typed.attrs.retain(|a| !a.path().is_ident("doc"));
        params.push(Param {
            ident: pattern.ident.clone(),
            ty: (*typed.ty).clone(),
            description,
        });
    }

    Ok(Tool {
        ident: function.sig.ident.clone(),
        name: function.sig.ident.to_string(),
        description,
        params,
        reply: reply(&function.sig.output)?,
        query,
        deprecated,
        public,
        callers,
    })
}

/// The `T` of a tool's `Result<T>`, or `None` for `()`.
fn reply(output: &ReturnType) -> syn::Result<Option<Type>> {
    let wrong = || {
        syn::Error::new_spanned(
            output,
            "a tool returns `berm_lang::Result<T>`, `T` being its reply",
        )
    };
    let ReturnType::Type(_, ty) = output else {
        return Err(wrong());
    };
    let Type::Path(path) = &**ty else {
        return Err(wrong());
    };
    let Some(segment) = path.path.segments.last() else {
        return Err(wrong());
    };
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return Err(wrong());
    };
    if segment.ident != "Result" {
        return Err(wrong());
    }
    let Some(GenericArgument::Type(reply)) = args.args.first() else {
        return Err(wrong());
    };
    Ok(match reply {
        Type::Tuple(unit) if unit.elems.is_empty() => None,
        reply => Some(reply.clone()),
    })
}

/// The function in the program's module that reads `tool`'s arguments, calls
/// it and writes its reply.
fn dispatch(tool: &Tool) -> Item {
    let name = format_ident!("{DISPATCH_PREFIX}{}", tool.name);
    let ident = &tool.ident;
    let idents: Vec<_> = tool.params.iter().map(|param| &param.ident).collect();
    let types = tool.params.iter().map(|param| &param.ty);
    let read = match idents.is_empty() {
        true => quote!(let _ = args;),
        false => quote! {
            #[derive(::berm_lang::serde::Deserialize)]
            #[serde(crate = "::berm_lang::serde")]
            struct __BermArgs {
                #(#idents: #types,)*
            }
            let __BermArgs { #(#idents),* } = ::berm_lang::dispatch::args(args)?;
        },
    };
    let call = quote!(#ident(#(#idents),*)?);
    let write = match tool.reply {
        Some(_) => quote!(::berm_lang::dispatch::reply(&#call)),
        None => quote! {
            #call;
            Ok(::berm_lang::__private::Vec::new())
        },
    };
    syn::parse_quote! {
        #[doc(hidden)]
        pub(crate) fn #name(
            args: &[u8],
        ) -> ::berm_lang::Result<::berm_lang::__private::Vec<u8>> {
            #read
            #write
        }
    }
}

/// A manifest as JSON text, with each tool's schemas left to the types that
/// own them.
struct Pieces {
    pieces: Vec<proc_macro2::TokenStream>,
    text: String,
}

impl Pieces {
    fn text(&mut self, text: &str) {
        self.text.push_str(text);
    }

    fn shape(&mut self, shape: proc_macro2::TokenStream) {
        self.flush();
        self.pieces
            .push(quote!(::berm_lang::schema::Piece::Shape(&#shape)));
    }

    fn flush(&mut self) {
        if !self.text.is_empty() {
            let text = std::mem::take(&mut self.text);
            self.pieces
                .push(quote!(::berm_lang::schema::Piece::Text(#text)));
        }
    }
}

/// The program's record in the manifest section.
fn manifest(config: &Config, tools: &[Tool]) -> Vec<proc_macro2::TokenStream> {
    let mut out = Pieces {
        pieces: Vec::new(),
        text: String::new(),
    };
    out.text(&format!(
        r#"{{"abi_version":4,"version":"{}","tools":["#,
        escape(&std::env::var("CARGO_PKG_VERSION").unwrap_or_default()),
    ));
    for (at, tool) in tools.iter().enumerate() {
        if at > 0 {
            out.text(",");
        }
        out.text(&format!(
            r#"{{"name":"{}","description":"{}","parameters":"#,
            escape(&tool.name),
            escape(&tool.description),
        ));
        match tool.params.is_empty() {
            true => out.text(DEFAULT_PARAMETERS),
            false => {
                let fields = tool.params.iter().map(|param| {
                    let name = param.ident.unraw().to_string();
                    let description = escape(&param.description);
                    let ty = &param.ty;
                    quote! {
                        ::berm_lang::schema::Field {
                            name: #name,
                            description: #description,
                            shape: &<#ty as ::berm_lang::Schema>::SHAPE,
                        }
                    }
                });
                out.shape(quote! {
                    ::berm_lang::schema::Shape::Object { title: "", fields: &[#(#fields),*] }
                });
            }
        }
        if let Some(reply) = &tool.reply {
            out.text(r#","returns":"#);
            out.shape(quote!(<#reply as ::berm_lang::Schema>::SHAPE));
        }
        if tool.query {
            out.text(r#","query":true"#);
        }
        if tool.deprecated {
            out.text(r#","deprecated":true"#);
        }
        if tool.public {
            out.text(r#","public":true"#);
        }
        if let Some(kind) = &tool.callers {
            out.text(&format!(r#","callers":"{kind}""#));
        }
        out.text("}");
    }

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
    let database = match &config.database {
        Some((_, migrations)) => format!(
            r#","database":{{"migrations":[{}]}}"#,
            migrations
                .iter()
                .map(|(name, sql)| format!(
                    r#"{{"name":"{}","sql":"{}"}}"#,
                    escape(name),
                    escape(sql)
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
        None => String::new(),
    };
    out.text(&format!(
        r#"],"usage":"{}","deps":{{"programs":[{programs}],"syscalls":{{{syscalls}}},"secrets":[{secrets}]}}{database}}}"#,
        escape(&config.usage),
    ));
    out.flush();
    out.pieces
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
