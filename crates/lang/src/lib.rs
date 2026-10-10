//! Build a Crabtalk program.
//!
//! A program is code the daemon schedules: one WebAssembly module — or one
//! RV64IMAC ELF, experimentally — confined to its own memory and reaching the
//! world only through the host calls it was given. This crate is what an author
//! writes against, and the same source builds for either: it owns the ABI so
//! they never see a call number, a register, or a pointer pair.
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
//! Build for `wasm32-unknown-unknown`, or for `riscv64imac-unknown-none-elf`
//! with `-Clink-arg=--emit-relocs`, which is not optional there. `berm new`
//! writes a crate whose `.cargo/config.toml` carries what its target needs.
//!
//! This crate declares the standard syscalls: [`call`] and [`after`] for
//! reaching programs, [`get`] and [`set`] for a program's own bytes, and
//! [`http::fetch`] for the network. berm serves [`call`] itself; the rest run
//! only under a host that registers them, and trap as unknown host calls
//! otherwise.

#![no_std]

// Off a guest target this is an ordinary library in someone's test binary,
// where std is both available and the point: it is what lets a stand-in host
// hold the argument blob and collect what a program logged.
#[cfg(not(any(target_arch = "wasm32", target_arch = "riscv64")))]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod abi;
mod call;
mod heap;
pub mod http;
mod out;
mod store;
pub mod test;
pub mod tool;

// One boundary for the whole crate: `wasm.rs` and `riscv.rs` make real host
// calls, and `stub.rs` is a host a test can stand in for. The split is about
// behaviour rather than what will compile, and it is what lets a program author
// run their tools without a guest around them.
//
// The two guest targets differ here and nowhere else. A syscall is the same
// number, the same request and the same staged reply on both; only how it
// leaves the guest changes — `ecall` on one, an import on the other.
#[cfg_attr(target_arch = "riscv64", path = "sys/riscv.rs")]
#[cfg_attr(target_arch = "wasm32", path = "sys/wasm.rs")]
#[cfg_attr(
    not(any(target_arch = "wasm32", target_arch = "riscv64")),
    path = "sys/stub.rs"
)]
mod sys;

pub use abi::{args_len, log, now};
pub use berm_codegen::{program, syscalls};
pub use out::Out;

#[cfg(feature = "alloc")]
pub use call::{CallError, after, call};
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

/// The ABI this SDK generates against. A host that does not recognise it
/// refuses the program rather than guessing.
pub const ABI_VERSION: u32 = 2;

#[doc(hidden)]
pub const ABI_VERSION_TEXT: &str = "2";

/// Returned by a handler that failed. Whatever it wrote to its [`Out`] becomes
/// the failure message, so an error can be specific without an allocator.
pub struct Failed;

/// Traps instead of looping. An author's panic reaches the host as a trap it
/// can report, rather than hanging the thread that called in.
///
/// Only on a guest target — off one, the crate is an ordinary library linked
/// into someone's tests, where std already supplies one.
#[cfg(any(target_arch = "wasm32", target_arch = "riscv64"))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // Say where before dying. A program author otherwise gets `guest executed
    // ebreak` and nothing else, which is the difference between a minute and
    // an afternoon.
    if let Some(location) = info.location() {
        abi::log(location.file());
    }
    sys::abort()
}
