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
//!     use berm_lang::{Failed, Out};
//!
//!     /// Echo the argument blob back.
//!     pub fn echo(args: &[u8], out: &mut Out) -> Result<(), Failed> {
//!         out.write(args);
//!         Ok(())
//!     }
//! }
//! ```
//!
//! Every `pub fn` in the module is a tool; its doc comment is what the model
//! reads when deciding whether to call it.
//!
//! Build for `wasm32-unknown-unknown`.
//!
//! This crate declares the standard syscalls: [`call`] and [`after`] for
//! reaching programs, [`get`] and [`set`] for a program's own bytes, [`emit`]
//! for logs ([`emit!`] for an `#[event]` struct), and [`http::fetch`] for the
//! network. berm serves [`call`] and
//! [`emit`] itself; the rest run only under a host that registers them, and
//! trap as unknown host calls otherwise.
//!
//! What [`set`] writes and [`emit`] emits is kept only if the call the host
//! started returns a result.

#![no_std]

// Off a guest target this is an ordinary library in someone's test binary,
// where std is both available and the point: it is what lets a stand-in host
// hold the argument blob and collect what a program traced.
#[cfg(not(target_arch = "wasm32"))]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod abi;
mod call;
mod emit;
mod heap;
pub mod http;
#[doc(hidden)]
pub mod json;
mod logs;
mod out;
mod store;
pub mod test;
pub mod tool;

// One boundary for the whole crate: `wasm.rs` makes real host calls, and
// `stub.rs` is a host a test can stand in for, which is what lets a program
// author run their tools without a guest around them.
#[cfg_attr(target_arch = "wasm32", path = "sys/wasm.rs")]
#[cfg_attr(not(target_arch = "wasm32"), path = "sys/stub.rs")]
mod sys;

pub use abi::{args_len, now, trace};
pub use berm_codegen::{declare_program, program, syscalls};
pub use out::Out;

#[cfg(feature = "alloc")]
pub use call::{CallError, after, call};
#[cfg(feature = "args")]
#[doc(hidden)]
pub use emit::event as __event;
#[cfg(feature = "alloc")]
pub use emit::{Event, emit};
#[cfg(feature = "args")]
pub use logs::{Delivery, Logged};
#[cfg(feature = "alloc")]
pub use store::{get, set};

// Re-exported so a program declares this SDK and nothing else. The `#[program]`
// macro writes `#[serde(crate = "::berm_lang::serde")]` onto argument structs,
// which only resolves if the author can reach serde through us — and an author
// who had to depend on it directly could pick a version that disagrees with
// the one the derive was generated against.
#[cfg(feature = "args")]
pub use serde_guest as serde;
#[cfg(feature = "args")]
pub use serde_json_guest as serde_json;

/// What `declare_program!`'s types are written in, so an author need not
/// declare `extern crate alloc` for them.
#[cfg(feature = "args")]
#[doc(hidden)]
pub mod __private {
    pub use alloc::{collections::BTreeMap, string::String, vec::Vec};
}

/// The ABI this SDK generates against. A host that does not recognise it
/// refuses the program rather than guessing.
pub const ABI_VERSION: u32 = 3;

#[doc(hidden)]
pub const ABI_VERSION_TEXT: &str = "3";

/// Returned by a handler that failed. Whatever it wrote to its [`Out`] becomes
/// the failure message, so an error can be specific without an allocator.
pub struct Failed;

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
