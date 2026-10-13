//! This program's own SQLite database, which its manifest declares with
//! `#[program(database = "migrations")]`.
//!
//! One statement per call. The host runs every statement of an invocation in
//! one transaction, committed when the invocation returns ok; transaction
//! statements are refused. Under a `query`, a statement that writes is refused.

use crate::abi::{self, host, host::CallError, wire};
use alloc::string::String;

/// Run `statement` with `params`, a JSON array bound to its `?` placeholders,
/// and answer `{"columns": [..], "rows": [[..]], "read": n, "written": n}`.
///
/// ```ignore
/// let found = berm_lang::sql("SELECT title FROM items WHERE id = ?", "[42]")?;
/// ```
pub fn sql(statement: &str, params: &str) -> Result<String, CallError> {
    let reply = host::call(
        abi::HOST_SQL,
        &wire::request(&[statement.as_bytes(), params.as_bytes()]),
    )?;
    String::from_utf8(reply)
        .map_err(|_| CallError::Failed(String::from("host answered SQL with invalid UTF-8")))
}
