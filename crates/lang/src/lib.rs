//! Build a Crabtalk program.
//!
//! A program is code the daemon schedules: one WebAssembly module, confined to
//! its own memory and reaching the world only through the host calls it was
//! given. This crate is what an author writes against: it owns the ABI so they
//! never see a call number or a pointer pair.
//!
//! ```ignore
//! #![no_std]
//! #![no_main]
//!
//! #[berm_lang::program]
//! mod tools {
//!     use berm_lang::Result;
//!
//!     /// Greets `name`.
//!     #[public]
//!     pub fn greet(
//!         /// Who to greet.
//!         name: alloc::string::String,
//!     ) -> Result<alloc::string::String> {
//!         Ok(alloc::format!("hello, {name}"))
//!     }
//! }
//! ```
//!
//! Every `pub fn` in the module is a tool; its doc comment is what the model
//! reads when deciding whether to call it. Its parameters are its arguments,
//! and the `Ok` of its [`Result`] is its reply, each typed by [`Schema`].
//!
//! Build for `wasm32-unknown-unknown`.
//!
//! This crate declares the standard syscalls: [`call`] and [`after`] for
//! reaching programs, [`get`] and [`set`] for a program's own bytes, [`emit`]
//! for logs ([`emit!`] for an `#[event]` struct), and [`http::fetch`] and
//! [`http::proxy`] for the network. berm serves [`call`] and [`emit`] itself;
//! the rest run only under a host that registers them, and trap as unknown host
//! calls otherwise.
//!
//! What [`set`] writes is kept only if the call the host started returns a
//! result. What [`emit`] emits is kept whatever the call returns: logs are what
//! its developer reads to debug it.

#![no_std]

// Off a guest target this is an ordinary library in someone's test binary,
// where std is both available and the point: it is what lets a stand-in host
// hold the argument blob and collect what a program traced.
#[cfg(not(target_arch = "wasm32"))]
extern crate std;

extern crate alloc;

// So `#[derive(Schema)]` resolves `::berm_lang` inside this crate too.
extern crate self as berm_lang;

pub mod abi;
mod call;
#[doc(hidden)]
pub mod dispatch;
mod emit;
mod error;
mod heap;
pub mod html;
pub mod http;
#[doc(hidden)]
pub mod json;
pub mod schema;
mod sql;
mod store;
pub mod test;

// One boundary for the whole crate: `wasm.rs` makes real host calls, and
// `stub.rs` is a host a test can stand in for, which is what lets a program
// author run their tools without a guest around them.
#[cfg_attr(target_arch = "wasm32", path = "sys/wasm.rs")]
#[cfg_attr(not(target_arch = "wasm32"), path = "sys/stub.rs")]
mod sys;

pub use abi::{args_len, now, trace};
pub use berm_codegen::{Schema, declare_program, error_code, event, program, syscalls};
pub use call::{CallError, Caller, User, after, call, caller, deny, user};
#[doc(hidden)]
pub use emit::event as __event;
pub use emit::{Event, emit};
pub use error::{Error, Result};
pub use schema::Schema;
pub use sql::sql;
pub use store::{get, get_json, set, set_json};

// Re-exported so a program declares this SDK and nothing else. What the macros
// generate reaches serde as `::berm_lang::serde`, which only resolves if the
// author can reach it through us — and an author who had to depend on it
// directly could pick a version that disagrees with the one the derive was
// generated against.
pub use serde_guest as serde;
pub use serde_json_guest as serde_json;

/// What `declare_program!`'s types are written in, so an author need not
/// declare `extern crate alloc` for them.
#[doc(hidden)]
pub mod __private {
    pub use alloc::{collections::BTreeMap, string::String, vec::Vec};
}

/// The ABI this SDK generates against. A host that does not recognise it
/// refuses the program rather than guessing.
pub const ABI_VERSION: u32 = 4;

#[doc(hidden)]
pub const ABI_VERSION_TEXT: &str = "4";

/// Traps instead of looping. An author's panic reaches the host as a trap it
/// can report, rather than hanging the thread that called in.
///
/// Only on a guest target — off one, the crate is an ordinary library linked
/// into someone's tests, where std already supplies one.
#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // Say where before dying. A program author otherwise gets a trap and
    // nothing else.
    if let Some(location) = info.location() {
        abi::trace(location.file());
    }
    sys::abort()
}
