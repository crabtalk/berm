//! Translates RV64IMAC into Cranelift IR.
//!
//! One CLIF function per RISC-V function. `jal` becomes a `call`, `ret`
//! becomes a native return, and the host's own stack carries return addresses.
//! Nothing here reconstructs a whole-program control-flow graph, because the
//! ELF already has one.
//!
//! ## How registers cross a call
//!
//! Guest registers live in CLIF variables inside a function. At a call
//! boundary only the ones the RISC-V ABI says are live get passed:
//!
//! ```text
//! fn(vmctx, sp, a0..a7) -> (sp, a0, a1)
//! ```
//!
//! Callee-saved registers stay in the caller's variables and are never handed
//! over. A callee that clobbers `s0` spills it to the guest stack in its own
//! prologue and reloads it in its epilogue, exactly as the hardware would --
//! those are real guest memory accesses that we honour. The value it spills is
//! its own zero-initialised `s0` rather than the caller's, which is
//! unobservable: nothing reads that slot except the epilogue that restores it.
//!
//! This assumes the guest honours the LP64 ABI. Compiler output does; hand
//! written assembly that passes data in `s0` or `t0` across a call does not.
//!
//! `gp` and `tp` are the exception. They are set once at startup and read
//! everywhere, so threading them through every signature would be wasteful and
//! dropping them would be wrong. They live in [`VmCtx`] and are loaded only by
//! functions that actually reference them.

use cranelift::{
    codegen::isa::CallConv,
    prelude::{AbiParam, Signature, types},
};

pub use crate::{
    analyze::{Analysis, Target, analyze},
    func::Imports,
    func::translate,
};

mod analyze;
mod func;
mod inst;

/// Guest state shared with compiled code.
///
/// Laid out for fixed-offset access from CLIF; see [`offsets`].
#[repr(C)]
pub struct VmCtx {
    /// The 32 general-purpose registers. Only `gp` and `tp` are read from here
    /// during normal execution; the rest live in CLIF variables.
    pub regs: [u64; 32],

    /// Base of the reserved guest address space.
    pub memory: *mut u8,

    /// Indirect-call dispatch table, indexed by `(target - text_base) >> 1`.
    pub dispatch: *const *const u8,

    /// Number of entries in `dispatch`.
    pub dispatch_len: u64,

    /// Base address of `.text`, the origin of the dispatch index.
    pub text_base: u64,

    /// `extern "C" fn(*mut VmCtx)`, invoked on `ecall`.
    pub host_call: *const u8,

    /// Opaque pointer to the embedder's state, read by the host call handler.
    pub host_data: *mut core::ffi::c_void,

    /// Points at a flag the host sets to stop the guest. Null when the module
    /// was not compiled interruptible.
    ///
    /// Read through a pointer rather than held inline so the handle stays valid
    /// when the store moves, and so a watchdog on another thread can share it.
    pub interrupt: *const u64,

    /// Set by compiled code before an abrupt return. See [`Trap`].
    pub trap: u64,
}

/// Byte offsets into [`VmCtx`], used by generated code.
pub mod offsets {
    /// Register file.
    pub const REGS: i32 = 0;
    /// Guest memory base.
    pub const MEMORY: i32 = 8 * 32;
    /// Dispatch table pointer.
    pub const DISPATCH: i32 = MEMORY + 8;
    /// Dispatch table length.
    pub const DISPATCH_LEN: i32 = DISPATCH + 8;
    /// `.text` base address.
    pub const TEXT_BASE: i32 = DISPATCH_LEN + 8;
    /// Host call trampoline pointer.
    pub const HOST_CALL: i32 = TEXT_BASE + 8;
    /// Embedder state pointer.
    pub const HOST_DATA: i32 = HOST_CALL + 8;
    /// Interrupt flag pointer.
    pub const INTERRUPT: i32 = HOST_DATA + 8;
    /// Trap code.
    pub const TRAP: i32 = INTERRUPT + 8;

    /// Offset of guest register `n`.
    pub const fn reg(n: usize) -> i32 {
        REGS + (n as i32) * 8
    }
}

/// Why compiled code returned early.
///
/// Zero means a normal return, so a freshly zeroed [`VmCtx`] reads as "no
/// trap" without extra initialisation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum Trap {
    /// The guest ran to completion.
    None = 0,
    /// An indirect jump landed somewhere that is not a known function entry.
    BadIndirectTarget = 1,
    /// The guest executed `ebreak`.
    Breakpoint = 2,
    /// A host call failed.
    HostCall = 3,
    /// The guest reached an `unimp`, which compilers place where control must
    /// not go.
    IllegalInstruction = 4,
    /// The host asked the guest to stop.
    Interrupted = 5,
}

impl Trap {
    /// Recover a trap code written by compiled code.
    pub fn from_code(code: u64) -> Self {
        match code {
            1 => Trap::BadIndirectTarget,
            2 => Trap::Breakpoint,
            3 => Trap::HostCall,
            4 => Trap::IllegalInstruction,
            5 => Trap::Interrupted,
            _ => Trap::None,
        }
    }
}

/// Calling convention for guest-to-guest calls.
///
/// `Fast` rather than a platform C convention: these signatures are wide (ten
/// parameters, three results) and are never called directly by the host, which
/// goes through a trampoline instead.
pub const GUEST_CALL_CONV: CallConv = CallConv::Fast;

/// Argument registers that survive a guest return.
///
/// A compiled function returns `(sp, a0, a1)`, so only `a0` and `a1` carry a
/// result back. Anything a caller reads beyond these two would be whatever the
/// register file held before the call, not a returned value.
pub const RESULT_REGS: usize = 2;

/// Parameter positions in a compiled guest function.
pub mod params {
    /// The [`super::VmCtx`] pointer.
    pub const VMCTX: usize = 0;
    /// The guest stack pointer.
    pub const SP: usize = 1;
    /// The first argument register, `a0`. `a1`..`a7` follow.
    pub const A0: usize = 2;
    /// How many argument registers are passed.
    pub const ARGS: usize = 8;
    /// Total parameter count.
    pub const COUNT: usize = A0 + ARGS;
}

/// The signature every compiled guest function shares.
pub fn signature() -> Signature {
    Signature {
        params: vec![AbiParam::new(types::I64); params::COUNT],
        returns: vec![AbiParam::new(types::I64); 3],
        call_conv: GUEST_CALL_CONV,
    }
}
