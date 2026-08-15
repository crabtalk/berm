//! Canonical RV64IMAC instruction form
//!
//! Compressed instructions decode into these same variants, so the translator
//! never sees the C extension. The only place width matters afterwards is
//! [`Inst::len`], which drives PC advancement and branch targets.

use crate::Reg;

/// A decoded instruction in canonical (uncompressed) form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inst {
    /// `lui rd, imm` — load upper immediate.
    Lui { rd: Reg, imm: i64 },
    /// `auipc rd, imm` — add upper immediate to PC.
    ///
    /// The translator resolves these to absolute addresses; see the constant
    /// propagation pass that pairs them with `jalr`.
    Auipc { rd: Reg, imm: i64 },
    /// `jal rd, imm` — jump and link, PC-relative.
    Jal { rd: Reg, imm: i64 },
    /// `jalr rd, imm(rs1)` — jump and link register.
    ///
    /// `rd == ra` is a call, `rd == zero && rs1 == ra && imm == 0` is a return,
    /// and `rd == zero` otherwise is a tail call or computed jump.
    Jalr { rd: Reg, rs1: Reg, imm: i64 },
    /// Conditional branch, PC-relative.
    Branch { op: Cond, rs1: Reg, rs2: Reg, imm: i64 },
    /// `l{b,h,w,d}[u] rd, imm(rs1)`
    Load { op: LoadOp, rd: Reg, rs1: Reg, imm: i64 },
    /// `s{b,h,w,d} rs2, imm(rs1)`
    Store { op: StoreOp, rs1: Reg, rs2: Reg, imm: i64 },
    /// Register-register ALU op on the full 64-bit width.
    Alu { op: AluOp, rd: Reg, rs1: Reg, rs2: Reg },
    /// Register-immediate ALU op on the full 64-bit width.
    AluImm { op: AluOp, rd: Reg, rs1: Reg, imm: i64 },
    /// Register-register ALU op on the low 32 bits, sign-extended to 64.
    AluW { op: AluOp, rd: Reg, rs1: Reg, rs2: Reg },
    /// Register-immediate ALU op on the low 32 bits, sign-extended to 64.
    AluImmW { op: AluOp, rd: Reg, rs1: Reg, imm: i64 },
    /// M-extension op on the full 64-bit width.
    Mul { op: MulOp, rd: Reg, rs1: Reg, rs2: Reg },
    /// M-extension op on the low 32 bits, sign-extended to 64.
    MulW { op: MulOp, rd: Reg, rs1: Reg, rs2: Reg },
    /// `amo<op>.{w,d} rd, rs2, (rs1)` — atomic read-modify-write.
    Amo { op: AmoOp, width: Width, rd: Reg, rs1: Reg, rs2: Reg, ord: Ordering },
    /// `lr.{w,d} rd, (rs1)` — load-reserved.
    LoadReserved { width: Width, rd: Reg, rs1: Reg, ord: Ordering },
    /// `sc.{w,d} rd, rs2, (rs1)` — store-conditional.
    StoreConditional { width: Width, rd: Reg, rs1: Reg, rs2: Reg, ord: Ordering },
    /// `fence` / `fence.i` — a no-op for a single-threaded guest.
    Fence,
    /// `ecall` — transfer to the host.
    Ecall,
    /// `ebreak` — breakpoint trap.
    Ebreak,
    /// `unimp` — the defined illegal instruction.
    ///
    /// The all-zero halfword is guaranteed by the ISA to trap, and compilers
    /// emit it deliberately where control must not reach: after a diverging
    /// call, or for an unreachable branch. It is a real instruction meaning
    /// "stop here", not undecodable input.
    Unimp,
}

impl Inst {
    /// Whether this instruction ends a basic block.
    pub fn is_terminator(&self) -> bool {
        matches!(
            self,
            Inst::Jal { .. } | Inst::Jalr { .. } | Inst::Branch { .. } | Inst::Ebreak | Inst::Unimp
        )
    }
}

/// Branch condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cond {
    /// `beq` — equal.
    Eq,
    /// `bne` — not equal.
    Ne,
    /// `blt` — signed less than.
    Lt,
    /// `bge` — signed greater or equal.
    Ge,
    /// `bltu` — unsigned less than.
    LtU,
    /// `bgeu` — unsigned greater or equal.
    GeU,
}

/// Load width and signedness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadOp {
    /// `lb` — 8-bit, sign-extended.
    B,
    /// `lh` — 16-bit, sign-extended.
    H,
    /// `lw` — 32-bit, sign-extended.
    W,
    /// `ld` — 64-bit.
    D,
    /// `lbu` — 8-bit, zero-extended.
    Bu,
    /// `lhu` — 16-bit, zero-extended.
    Hu,
    /// `lwu` — 32-bit, zero-extended.
    Wu,
}

/// Store width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreOp {
    /// `sb` — 8-bit.
    B,
    /// `sh` — 16-bit.
    H,
    /// `sw` — 32-bit.
    W,
    /// `sd` — 64-bit.
    D,
}

/// Base integer ALU operation.
///
/// `Sub` and `Sra` never appear with an immediate; `addi` with a negated
/// immediate covers subtraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AluOp {
    /// `add`
    Add,
    /// `sub`
    Sub,
    /// `sll` — shift left logical.
    Sll,
    /// `slt` — set if signed less than.
    Slt,
    /// `sltu` — set if unsigned less than.
    SltU,
    /// `xor`
    Xor,
    /// `srl` — shift right logical.
    Srl,
    /// `sra` — shift right arithmetic.
    Sra,
    /// `or`
    Or,
    /// `and`
    And,
}

/// M-extension operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MulOp {
    /// `mul` — low half of the product.
    Mul,
    /// `mulh` — high half, signed × signed.
    MulH,
    /// `mulhsu` — high half, signed × unsigned.
    MulHSU,
    /// `mulhu` — high half, unsigned × unsigned.
    MulHU,
    /// `div` — signed division.
    Div,
    /// `divu` — unsigned division.
    DivU,
    /// `rem` — signed remainder.
    Rem,
    /// `remu` — unsigned remainder.
    RemU,
}

/// A-extension read-modify-write operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmoOp {
    /// `amoswap`
    Swap,
    /// `amoadd`
    Add,
    /// `amoxor`
    Xor,
    /// `amoand`
    And,
    /// `amoor`
    Or,
    /// `amomin` — signed minimum.
    Min,
    /// `amomax` — signed maximum.
    Max,
    /// `amominu` — unsigned minimum.
    MinU,
    /// `amomaxu` — unsigned maximum.
    MaxU,
}

/// Width of an atomic access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
    /// 32-bit, result sign-extended to 64.
    W,
    /// 64-bit.
    D,
}

/// Acquire/release annotation on an atomic.
///
/// A single-threaded guest cannot observe the difference, so the translator
/// ignores this; it is decoded only so disassembly round-trips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ordering {
    /// The `aq` bit.
    pub acquire: bool,
    /// The `rl` bit.
    pub release: bool,
}
