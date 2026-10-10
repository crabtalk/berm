//! The reference guest: the smallest real program, and what berm is measured
//! and tested against.
//!
//! Everything below the `#[program]` line is what an author actually writes.
//! The exports, the manifest section, the dispatch, and the panic handler come
//! from the SDK.
//!
//! Each tool prices or proves one thing, which is why they are not useful on
//! their own: `echo` carries typed arguments across the boundary, `typed`
//! deserializes the same payload to price a parse, `chatty` makes a hundred
//! host calls to price one, `probe` allocates to show the heap arrives without
//! a second entry into the guest, and `boom` fails on purpose.
//! `tick` arms itself, to show a wake outliving the invocation that asked for
//! it, and `wait` makes one host call a host can make slow.
//! `crates/berm/examples/measure.rs` and `load.rs` read the numbers off them,
//! and `tests/tools.rs` is the only exercise the SDK's host-side `test::call`
//! gets.

// `no_std` and `no_main` are the guest's shape. Off a guest target this is an
// ordinary library, so `cargo test` runs the tools below natively.
#![cfg_attr(any(target_arch = "wasm32", target_arch = "riscv64"), no_std, no_main)]

extern crate alloc;

// `inner` is what `nest` and `recurse` reach for. It is not resolved here,
// only declared.
#[berm_lang::program(
    programs = ["inner"],
    syscalls = { "berm.get", "berm.set", "berm.call.after" },
)]
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

    /// Reads one stored key and answers whether it was set. Under
    /// `examples/load.rs` the read stands in for a host call that waits on
    /// I/O.
    pub fn wait(_args: &[u8], out: &mut Out) -> Result<(), Failed> {
        match berm_lang::get("wait") {
            Ok(Some(_)) => out.write(b"set"),
            Ok(None) => out.write(b"unset"),
            Err(_) => return Err(Failed),
        }
        Ok(())
    }

    /// Always fails, to exercise the error path.
    pub fn boom(_args: &[u8], out: &mut Out) -> Result<(), Failed> {
        out.write(b"boom, as requested");
        Err(Failed)
    }

    /// Calls `echo` on whatever is deployed as `inner`, to prove a program can
    /// reach a program. Says which kind of failure came back, because telling
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
            Err(error) => berm_lang::tool::syscall(Err(error), out).map(|_: ()| ()),
        }
    }

    /// Counts how many times it has run, arming itself to run again in the
    /// milliseconds its arguments name until it has run three times. Proves an
    /// invocation can outlive the one that asked for it, and that what crosses
    /// the gap is the program's own keys.
    pub fn tick(args: &[u8], out: &mut Out) -> Result<(), Failed> {
        let delay = core::str::from_utf8(args)
            .unwrap_or("")
            .trim()
            .parse()
            .unwrap_or(0);

        let round = berm_lang::get("ticks")
            .ok()
            .flatten()
            .and_then(|seen| core::str::from_utf8(&seen).ok()?.parse::<u64>().ok())
            .unwrap_or(0)
            + 1;

        let mut buffer = alloc::string::String::new();
        let _ = core::fmt::Write::write_fmt(&mut buffer, format_args!("{round}"));
        if berm_lang::set("ticks", buffer.as_bytes()).is_err() {
            return Err(Failed);
        }

        // The delay rides along, so each round arms the next on the same one.
        let mut again = alloc::string::String::new();
        let _ = core::fmt::Write::write_fmt(&mut again, format_args!("{delay}"));
        if round < 3 && berm_lang::after(delay, "fixture", "tick", &again).is_err() {
            return Err(Failed);
        }

        let mut said = alloc::string::String::new();
        let _ = core::fmt::Write::write_fmt(
            &mut said,
            format_args!(r#"{{"round":{round},"at":{}}}"#, berm_lang::now()),
        );
        out.write(said.as_bytes());
        Ok(())
    }

    /// Calls itself on `inner`, which is this same program when deployed under
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
            Err(error) => berm_lang::tool::syscall(Err(error), out).map(|_: ()| ()),
        }
    }
}
