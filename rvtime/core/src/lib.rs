//! Shared vocabulary for rvtime: registers, instructions, and loaded programs.
//!
//! This crate decodes but never generates code. Backends depend on it; it
//! depends on no backend.

pub use crate::{
    decode::decode,
    inst::{AluOp, AmoOp, Cond, Inst, LoadOp, MulOp, Ordering, StoreOp, Width},
    program::{Function, Perms, Program, Segment},
    reg::{REGISTER_ARGS, REGISTER_COUNT, Reg},
};

mod decode;
mod inst;
mod program;
mod reg;

pub mod elf;

/// Guest page size.
pub const PAGE_SIZE: u64 = 4096;

/// Default size of the guest address space.
///
/// Small enough not to strain a host's address space when many guests run at
/// once, and far above what a typical freestanding image needs -- the usual
/// RISC-V link places the program at `0x10000`. Raise it with
/// `Config::memory_size` for a program that needs more.
pub const DEFAULT_MEMORY_SIZE: u64 = 64 << 20;

/// Smallest permitted guest address space.
pub const MIN_MEMORY_SIZE: u64 = 64 << 10;

/// Largest permitted guest address space.
///
/// Guest addresses are confined by masking, and the mask is only meaningful
/// within a 64-bit word; 4 GiB also keeps a whole address space reservable in
/// one `mmap`.
pub const MAX_MEMORY_SIZE: u64 = 1 << 32;

/// Check that a guest address space size can be used.
///
/// It must be a power of two, because confinement is a single bitwise `and`
/// against `size - 1`. Any other size would leave addresses between the mask's
/// range and the end of the reservation pointing at unmapped host memory.
pub fn check_memory_size(size: u64) -> anyhow::Result<()> {
    if !size.is_power_of_two() {
        anyhow::bail!("guest memory size {size:#x} must be a power of two");
    }
    if !(MIN_MEMORY_SIZE..=MAX_MEMORY_SIZE).contains(&size) {
        anyhow::bail!(
            "guest memory size {size:#x} must be between {MIN_MEMORY_SIZE:#x} and {MAX_MEMORY_SIZE:#x}"
        );
    }
    Ok(())
}
