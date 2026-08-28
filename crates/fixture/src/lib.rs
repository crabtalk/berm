//! The reference guest: the smallest real harness, and what berm is measured
//! and tested against.
//!
//! Everything below the `#[harness]` line is what an author actually writes.
//! The exports, the manifest section, the dispatch, and the panic handler come
//! from the SDK.
//!
//! Each tool prices or proves one thing, which is why they are not useful on
//! their own: `echo` carries typed arguments across the boundary, `typed`
//! deserializes the same payload to price a parse, `chatty` makes a hundred
//! host calls to price one, `probe` allocates to show the heap arrives without
//! a second entry into the guest, and `boom` fails on purpose.
//! `crates/berm/examples/measure.rs` reads the numbers off them, and
//! `tests/tools.rs` is the only exercise the SDK's host-side `test::call` gets.

// `no_std` and `no_main` are the guest's shape. Off its target this is an
// ordinary library, so `cargo test` runs the tools below natively.
#![cfg_attr(target_arch = "riscv64", no_std, no_main)]

extern crate alloc;

#[berm_lang::harness]
mod tools {
    use berm_lang::{Failed, Out};

    /// Echo the argument blob back inside a JSON envelope.
    #[args(Echo)]
    pub fn echo(args: &[u8], out: &mut Out) -> Result<(), Failed> {
        out.write(br#"{"echo":"#);
        out.write(args);
        out.write(b"}");
        Ok(())
    }

    /// Arguments for `echo`.
    pub struct Echo {
        /// The text to echo back.
        pub query: &'static str,
        /// Page number, zero-indexed.
        pub page: Option<u32>,
    }

    /// Makes 100 host calls, to price one.
    pub fn chatty(_args: &[u8], out: &mut Out) -> Result<(), Failed> {
        let mut total = 0;
        for _ in 0..100 {
            total += berm_lang::args_len();
        }
        if total == usize::MAX {
            return Err(Failed);
        }
        out.write(b"ok");
        Ok(())
    }

    /// Deserializes its arguments, to price a JSON parse against `echo`, which
    /// carries the same payload without reading it.
    #[args(Typed)]
    pub fn typed(args: &[u8], out: &mut Out) -> Result<(), Failed> {
        let parsed: Typed = berm_lang::tool::parse(args, out)?;
        out.write(parsed.query.as_bytes());
        Ok(())
    }

    /// Arguments for `typed`.
    pub struct Typed {
        /// The text to read back.
        pub query: alloc::string::String,
        /// Page number, zero-indexed.
        pub page: Option<u32>,
    }

    /// Allocates, to prove the heap arrives without a second entry.
    pub fn probe(_args: &[u8], out: &mut Out) -> Result<(), Failed> {
        let v = alloc::vec![7u8; 4096];
        out.write(&v[..2]);
        Ok(())
    }

    /// Always fails, to exercise the error path.
    pub fn boom(_args: &[u8], out: &mut Out) -> Result<(), Failed> {
        out.write(b"boom, as requested");
        Err(Failed)
    }

    /// Calls `echo` on whatever is deployed as `inner`, to prove a harness can
    /// reach a harness. Says which kind of failure came back, because telling
    /// "it ran and said no" from "it was never there" is the point of the
    /// second bit on the wire.
    pub fn nest(args: &[u8], out: &mut Out) -> Result<(), Failed> {
        let args = core::str::from_utf8(args).unwrap_or("{}");
        match berm_lang::call("inner", "echo", args) {
            Ok(result) => {
                out.write(b"nested:");
                out.write(result.as_bytes());
                Ok(())
            }
            Err(error) if error.refused() => {
                out.write(b"refused: ");
                out.write(error.message().as_bytes());
                Err(Failed)
            }
            Err(error) => berm_lang::tool::system(Err(error), out).map(|_: ()| ()),
        }
    }

    /// Dials the URL in its arguments, pointing everything that happens on it
    /// at `wire` below. Answers with the connection's id.
    pub fn dial(args: &[u8], out: &mut Out) -> Result<(), Failed> {
        let url = core::str::from_utf8(args).unwrap_or("").trim();
        match berm_lang::socket::open(url, "fixture", "wire") {
            Ok(id) => {
                let mut buffer = alloc::string::String::new();
                let _ = core::fmt::Write::write_fmt(&mut buffer, format_args!("{id}"));
                out.write(buffer.as_bytes());
                Ok(())
            }
            Err(error) => berm_lang::tool::system(Err(error), out).map(|_: ()| ()),
        }
    }

    /// Where `dial`'s connection lands, echoing each frame back down the one it
    /// arrived on — the whole loop in one tool: a host event starts an
    /// invocation, and the invocation answers the connection that caused it.
    pub fn wire(args: &[u8], out: &mut Out) -> Result<(), Failed> {
        use berm_lang::socket::{Kind, event, send};

        let Some(event) = event(args) else {
            out.write(b"not a connection event");
            return Err(Failed);
        };
        match event.kind {
            Kind::Open(failure) if failure.is_empty() => out.write(b"open"),
            Kind::Open(failure) => {
                out.write(b"dial failed: ");
                out.write(failure.as_bytes());
                return Err(Failed);
            }
            Kind::Message(frame) => {
                send(event.connection, frame)
                    .map_err(|error| berm_lang::tool::system::<()>(Err(error), out).unwrap_err())?;
                out.write(b"echoed");
            }
            Kind::Close(_) => out.write(b"closed"),
        }
        Ok(())
    }

    /// Calls itself on `inner`, which is this same harness when deployed under
    /// that name — the runaway a depth limit exists to stop. Reports how many
    /// levels got through before the host refused.
    pub fn recurse(args: &[u8], out: &mut Out) -> Result<(), Failed> {
        let args = core::str::from_utf8(args).unwrap_or("0");
        let depth: u32 = args.trim().parse().unwrap_or(0);

        let mut buffer = alloc::string::String::new();
        let _ = core::fmt::Write::write_fmt(&mut buffer, format_args!("{}", depth + 1));

        match berm_lang::call("inner", "recurse", &buffer) {
            Ok(deeper) => {
                out.write(deeper.as_bytes());
                Ok(())
            }
            // The bottom of the chain: the host refused to go further, and the
            // depth reached rides back up as the result.
            Err(error) if error.refused() => {
                out.write(buffer.as_bytes());
                Ok(())
            }
            Err(error) => berm_lang::tool::system(Err(error), out).map(|_: ()| ()),
        }
    }
}
