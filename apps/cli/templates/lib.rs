//! __NAME__ — a berm harness.

// `no_std` and `no_main` are the guest's shape. Off its target this is an
// ordinary library, so `cargo test` runs the tools below natively.
#![cfg_attr(target_arch = "riscv64", no_std, no_main)]

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
    }
}
