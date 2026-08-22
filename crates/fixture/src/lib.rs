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
}
