//! The reference guest keeping a database: its `berm.get`, `berm.set` and
//! `berm.emit` land in the database beside its own `words` table.
//!
//! `put` writes a word every way the database takes one, `count` reads the
//! table back, and `relay` calls both on `inner`.

#![cfg_attr(target_arch = "wasm32", no_std, no_main)]

extern crate alloc;

#[berm_lang::program(
    database = "migrations",
    programs = ["inner"],
    syscalls = { "berm.get", "berm.set", "berm.sql" },
)]
mod tools {
    use berm_lang::{Failed, Out};

    /// Inserts its argument into `words`, writes it under `last` and emits it
    /// under the topic `put`.
    #[public]
    pub fn put(args: &[u8], out: &mut Out) -> Result<(), Failed> {
        let word = core::str::from_utf8(args).unwrap_or("").trim();
        let Ok(params) = berm_lang::serde_json::to_string(&[word]) else {
            return berm_lang::tool::failed("the word does not serialize", out);
        };
        berm_lang::tool::syscall(
            berm_lang::sql("INSERT INTO words (word) VALUES (?)", &params),
            out,
        )?;
        berm_lang::tool::syscall(berm_lang::set("last", word.as_bytes()), out)?;
        berm_lang::tool::syscall(berm_lang::emit(&["put"], word), out)?;
        out.write(b"ok");
        Ok(())
    }

    /// Answers how many rows `words` holds, as `berm.sql` answers it.
    #[query]
    #[public]
    pub fn count(_args: &[u8], out: &mut Out) -> Result<(), Failed> {
        let counted =
            berm_lang::tool::syscall(berm_lang::sql("SELECT COUNT(*) FROM words", "[]"), out)?;
        out.write(counted.as_bytes());
        Ok(())
    }

    /// Writes its argument under `last`, calls `put` on `inner` with it, then
    /// `count` on `inner`, and answers what `count` did.
    #[public]
    pub fn relay(args: &[u8], out: &mut Out) -> Result<(), Failed> {
        let word = core::str::from_utf8(args).unwrap_or("").trim();
        berm_lang::tool::syscall(berm_lang::set("last", word.as_bytes()), out)?;
        berm_lang::tool::syscall(berm_lang::call("inner", "put", word), out)?;
        let counted = berm_lang::tool::syscall(berm_lang::call("inner", "count", ""), out)?;
        out.write(counted.as_bytes());
        Ok(())
    }
}
