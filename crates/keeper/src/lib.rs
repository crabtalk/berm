//! The reference guest keeping a database: its `berm.get` and `berm.set` land
//! in the database beside its own `words` table.
//!
//! `put` writes a word every way the database takes one, `count` reads the
//! table back, and `relay` calls both on `inner`.

#![cfg_attr(target_arch = "wasm32", no_std, no_main)]

extern crate alloc;

use alloc::vec::Vec;
use berm_lang::Schema;

#[berm_lang::program(
    database = "migrations",
    programs = ["inner"],
    syscalls = { "berm.get", "berm.set", "berm.sql" },
)]
mod tools {
    use crate::Counted;
    use alloc::{format, string::String};
    use berm_lang::Result;

    /// Inserts `word` into `words`, writes it under `last` and emits it under
    /// the topic `put`.
    #[public]
    pub fn put(word: String) -> Result<()> {
        let params = berm_lang::json::to(&[&word])?;
        berm_lang::sql("INSERT INTO words (word) VALUES (?)", &params)?;
        berm_lang::set("last", word.as_bytes())?;
        berm_lang::emit(&["put"], &word)?;
        Ok(())
    }

    /// Answers how many rows `words` holds.
    #[query]
    #[public]
    pub fn count() -> Result<u64> {
        let answer = berm_lang::sql("SELECT COUNT(*) FROM words", "[]")?;
        let Counted { rows } = berm_lang::json::from(&answer)?;
        Ok(rows
            .first()
            .and_then(|row| row.first())
            .copied()
            .unwrap_or(0))
    }

    /// Writes `word` under `last`, calls `put` on `inner` with it, then `count`
    /// on `inner`, and answers what `count` did.
    #[public]
    pub fn relay(word: String) -> Result<u64> {
        berm_lang::set("last", word.as_bytes())?;
        let args = format!(r#"{{"word":{}}}"#, berm_lang::json::to(&word)?);
        berm_lang::call("inner", "put", &args)?;
        let counted = berm_lang::call("inner", "count", "")?;
        Ok(berm_lang::json::from(&counted)?)
    }
}

/// The rows of what `berm.sql` answers a count with.
#[derive(Schema)]
pub struct Counted {
    pub rows: Vec<Vec<u64>>,
}
