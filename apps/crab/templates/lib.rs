//! __NAME__ — a berm program.

// `no_std` and `no_main` are the guest's shape. Off a guest target this is an
// ordinary library, so `cargo test` runs the tools below natively.
#![cfg_attr(target_arch = "wasm32", no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use berm_lang::Schema;

#[berm_lang::program]
mod tools {
    use crate::Echo;
    use alloc::string::String;
    use berm_lang::Result;

    /// Answers its argument back.
    #[public]
    pub fn echo(
        /// The text to echo back.
        query: String,
    ) -> Result<Echo> {
        Ok(Echo { query })
    }
}

/// What `echo` answers.
#[derive(Schema)]
pub struct Echo {
    pub query: String,
}
