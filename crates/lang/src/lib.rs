//! Build a Crabtalk program.
//!
//! A program is code the daemon schedules: one RV64IMAC ELF, confined to its
//! own address space, reaching the world only through the host calls it was
//! given. This crate is what an author writes against — it owns the ABI so
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
//! Build for `riscv64imac-unknown-none-elf` with `-Clink-arg=--emit-relocs`,
//! neither of which is optional. `berm new` writes a crate whose
//! `.cargo/config.toml` carries the flag.
//!
//! This crate declares one syscall and no more: [`call`], for reaching
//! another program the same host is running. That one is about the program
//! model itself rather than about any host's world, which is why it is here.
//! What a program can reach *outside* is whatever its host registered, and
//! naming any of that here would be the SDK deciding what a host must serve. A
//! program running under the Crabtalk daemon adds
//! [`berm-crabtalk`](https://crates.io/crates/berm-crabtalk), which declares
//! that namespace — files, commands, HTTP, the runtime.

#![no_std]

// Off the guest's target this is an ordinary library in someone's test binary,
// where std is both available and the point: it is what lets a stand-in host
// hold the argument blob and collect what a program logged.
#[cfg(not(target_arch = "riscv64"))]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod abi;
mod call;
mod heap;
mod out;
pub mod socket;
mod store;
pub mod test;
pub mod tool;

// One boundary for the whole crate: `riscv.rs` makes real host calls, and
// `stub.rs` is a host a test can stand in for. The split is about behaviour
// rather than what will compile — `rvtime-guest` builds everywhere now — and it
// is what lets a program author run their tools without a guest around them.
#[cfg_attr(target_arch = "riscv64", path = "sys/riscv.rs")]
#[cfg_attr(not(target_arch = "riscv64"), path = "sys/stub.rs")]
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
pub const ABI_VERSION: u32 = 1;

#[doc(hidden)]
pub const ABI_VERSION_TEXT: &str = "1";

/// Returned by a handler that failed. Whatever it wrote to its [`Out`] becomes
/// the failure message, so an error can be specific without an allocator.
pub struct Failed;

/// Traps instead of looping. An author's panic reaches the host as a
/// breakpoint it can report, rather than hanging the thread that called in.
///
/// Only on the guest's own target — off it, the crate is an ordinary library
/// linked into someone's tests, where std already supplies one.
#[cfg(target_arch = "riscv64")]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // Say where before dying. A program author otherwise gets `guest executed
    // ebreak` and nothing else, which is the difference between a minute and
    // an afternoon.
    if let Some(location) = info.location() {
        abi::log(location.file());
    }
    guest::abort()
}
