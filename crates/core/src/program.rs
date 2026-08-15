//! A loaded program, decoded and ready for translation

use crate::Inst;
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

/// A loaded RISC-V program.
///
/// Everything a backend needs and nothing about how it will be compiled.
#[derive(Debug)]
pub struct Program {
    /// Loadable segments, in address order.
    pub segments: Vec<Segment>,

    /// The ELF entry point.
    pub entry: u64,

    /// The executable range, used to decide whether an address is code.
    pub text: Range<u64>,

    /// Functions by start address.
    pub functions: BTreeMap<u64, Function>,

    /// Addresses that an indirect jump may land on.
    ///
    /// Derived from relocations rather than guessed: a `R_RISCV_64` pointing
    /// into [`Program::text`] is a function whose address was taken.
    pub indirect: BTreeSet<u64>,

    /// Exported symbol addresses by name, for `get_typed_func`.
    pub symbols: BTreeMap<String, u64>,
}

impl Program {
    /// The address just past the highest byte the image occupies.
    ///
    /// A guest address space has to be at least this large, with room for the
    /// stack above it.
    pub fn image_end(&self) -> u64 {
        self.segments
            .iter()
            .map(|s| s.addr + s.size)
            .max()
            .unwrap_or(0)
    }

    /// The function containing `addr`, if any.
    pub fn function_at(&self, addr: u64) -> Option<&Function> {
        self.functions
            .range(..=addr)
            .next_back()
            .map(|(_, f)| f)
            .filter(|f| f.range.contains(&addr))
    }
}

/// A function recovered from the symbol table.
#[derive(Debug)]
pub struct Function {
    /// The symbol name.
    pub name: String,

    /// The address range the function occupies.
    pub range: Range<u64>,

    /// Decoded instructions, paired with their addresses.
    pub code: Vec<(u64, Inst)>,
}

/// A loadable segment of the guest image.
#[derive(Debug)]
pub struct Segment {
    /// Guest virtual address.
    pub addr: u64,

    /// Initialised contents. May be shorter than `size` when `.bss` follows.
    pub data: Vec<u8>,

    /// Total size in memory, including any zero-filled tail.
    pub size: u64,

    /// Page permissions to apply once the contents are in place.
    pub perms: Perms,
}

/// Segment permissions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Perms {
    /// Readable.
    pub read: bool,
    /// Writable.
    pub write: bool,
    /// Executable.
    pub exec: bool,
}
