//! Control-flow analysis of a single function
//!
//! Two things have to be settled before any CLIF is emitted: where the basic
//! blocks start, and what each control transfer actually does.
//!
//! The second is the harder one. LLVM materialises a call target with
//! `auipc ra, hi` followed by `jalr ra, lo(ra)`, so on RISC-V a *direct* call
//! is encoded as an indirect jump. Compiling every `jalr` as an indirect
//! dispatch would be correct and catastrophically slow, so this pass tracks
//! constant register values within each block and folds the pair back into the
//! address it was always going to compute.
//!
//! Matching on adjacency would be simpler and wrong: LLVM is free to schedule
//! other instructions between the two, and linker relaxation can collapse the
//! pair into a bare `jal`.

use rv::{AluOp, Function, Inst, Reg};
use std::collections::{BTreeMap, BTreeSet};

/// What a control transfer resolves to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A jump to another block in the same function.
    Local(u64),

    /// A call to a known function.
    Direct {
        /// Entry address of the callee.
        addr: u64,
        /// Whether the caller's frame is abandoned (`rd` is `zero`).
        tail: bool,
    },

    /// A call through a computed address.
    Indirect {
        /// Whether the caller's frame is abandoned (`rd` is `zero`).
        tail: bool,
    },

    /// `jalr zero, 0(ra)` -- a return.
    Return,
}

/// The result of analysing one function.
#[derive(Debug, Default)]
pub struct Analysis {
    /// Addresses that start a basic block.
    pub leaders: BTreeSet<u64>,

    /// Relocation-named addresses *inside* this function, which is what a jump
    /// table's entries are. A computed jump may land on one, and it is a local
    /// jump rather than a call — nothing else in the program may target them.
    pub table: BTreeSet<u64>,

    /// Resolved control transfers, keyed by the address of the instruction.
    pub targets: BTreeMap<u64, Target>,

    /// Entry addresses this function calls directly, which the module must
    /// declare before translation so the calls can be linked.
    pub calls: BTreeSet<u64>,

    /// Whether the function reads `gp` or `tp`, which live in the VM context
    /// rather than being threaded through call signatures.
    pub reads_globals: bool,

    /// Whether the function writes `gp` or `tp`, in which case the values must
    /// be flushed back to the VM context before returning.
    pub writes_globals: bool,

    /// Whether any transfer jumps backwards, i.e. the function contains a loop.
    ///
    /// A guest can only run forever by looping — unbounded recursion exhausts
    /// the stack and traps — so a check on every backward edge is enough to
    /// make any guest interruptible.
    pub has_backedge: bool,
}

/// Analyse a function's control flow.
///
/// `entries` is the set of addresses that begin a function anywhere in the
/// program. It is what separates a call from a local jump: `rd == ra` is not
/// sufficient, because a `jal ra, <local label>` targets an address inside the
/// current function, and recursion targets an address that is both a function
/// entry and inside the current function.
///
/// `indirect` is every address a relocation names as reachable by a computed
/// jump. Those falling inside this function are its jump tables.
pub fn analyze(function: &Function, entries: &BTreeSet<u64>, indirect: &BTreeSet<u64>) -> Analysis {
    let mut analysis = Analysis::default();
    analysis.leaders.insert(function.range.start);

    // An address the linker named *and* placed inside this function is a jump
    // table entry. The function's own start is excluded: a computed call that
    // recurses is an ordinary indirect call, and it already has a dispatch slot.
    analysis.table = indirect
        .range(function.range.start + 1..function.range.end)
        .copied()
        .collect();
    analysis.leaders.extend(&analysis.table);

    leaders(function, &mut analysis);
    resolve(function, entries, &mut analysis);
    globals(function, &mut analysis);

    analysis
}

/// Find basic block boundaries.
///
/// Every branch and PC-relative jump has a statically known target, so this
/// needs no constant tracking. A computed jump's own targets are not found
/// here — they cannot be, since the address is computed — but they are leaders
/// all the same, which is why [`analyze`] seeds them from the relocations
/// before this runs.
fn leaders(function: &Function, analysis: &mut Analysis) {
    let range = &function.range;

    for (index, (addr, inst)) in function.code.iter().enumerate() {
        let next = function
            .code
            .get(index + 1)
            .map(|(a, _)| *a)
            .unwrap_or(range.end);

        match inst {
            Inst::Branch { imm, .. } => {
                let target = addr.wrapping_add(*imm as u64);
                if range.contains(&target) {
                    analysis.leaders.insert(target);
                }
                // The not-taken path begins a block of its own.
                if range.contains(&next) {
                    analysis.leaders.insert(next);
                }
            }
            Inst::Jal { rd, imm } => {
                let target = addr.wrapping_add(*imm as u64);
                if rd.is_zero() && range.contains(&target) {
                    analysis.leaders.insert(target);
                }
                if range.contains(&next) {
                    analysis.leaders.insert(next);
                }
            }
            Inst::Jalr { .. } if range.contains(&next) => {
                analysis.leaders.insert(next);
            }
            _ => {}
        }
    }
}

/// Resolve each control transfer, folding `auipc`/`jalr` pairs into calls.
fn resolve(function: &Function, entries: &BTreeSet<u64>, analysis: &mut Analysis) {
    let range = &function.range;

    // A known target is a call if it starts a function, and a local jump
    // otherwise. Entries are checked first so that recursion -- whose target
    // is both an entry and inside the current function -- stays a call.
    let classify = |dest: u64, tail: bool| {
        if entries.contains(&dest) {
            Target::Direct { addr: dest, tail }
        } else if range.contains(&dest) {
            Target::Local(dest)
        } else {
            Target::Direct { addr: dest, tail }
        }
    };

    // Constant register values, reset at every block boundary because a block
    // with multiple predecessors cannot assume any incoming value.
    let mut known = Known::default();

    for (addr, inst) in &function.code {
        if analysis.leaders.contains(addr) {
            known = Known::default();
        }

        let target = match inst {
            // For a branch this records the taken path; the fall-through is
            // implicit in the block order.
            Inst::Branch { imm, .. } => {
                let dest = addr.wrapping_add(*imm as u64);
                if range.contains(&dest) {
                    Some(Target::Local(dest))
                } else {
                    // A branch leaving the function is a conditional tail call.
                    Some(Target::Direct {
                        addr: dest,
                        tail: true,
                    })
                }
            }
            Inst::Jal { rd, imm } => {
                let dest = addr.wrapping_add(*imm as u64);
                Some(classify(dest, rd.is_zero()))
            }
            Inst::Jalr { rd, rs1, imm } => {
                if rd.is_zero() && *rs1 == Reg::RA && *imm == 0 {
                    Some(Target::Return)
                } else if let Some(base) = known.get(*rs1) {
                    // The low bit is cleared by the architecture.
                    let dest = base.wrapping_add(*imm as u64) & !1;
                    Some(classify(dest, rd.is_zero()))
                } else {
                    Some(Target::Indirect { tail: rd.is_zero() })
                }
            }
            _ => None,
        };

        if let Some(target) = target {
            match target {
                Target::Direct { addr: dest, .. } => {
                    analysis.calls.insert(dest);
                }
                Target::Local(dest) if dest <= *addr => {
                    analysis.has_backedge = true;
                }
                _ => {}
            }
            analysis.targets.insert(*addr, target);
        }

        known.step(*addr, inst);
    }
}

/// Note whether the function touches `gp` or `tp`.
fn globals(function: &Function, analysis: &mut Analysis) {
    for (_, inst) in &function.code {
        for reg in reads(inst) {
            if reg == Reg::GP || reg == Reg::TP {
                analysis.reads_globals = true;
            }
        }
        if let Some(rd) = writes(inst)
            && (rd == Reg::GP || rd == Reg::TP)
        {
            analysis.writes_globals = true;
        }
    }
}

/// Constant register values tracked within a basic block.
#[derive(Default)]
struct Known([Option<u64>; 32]);

impl Known {
    fn get(&self, reg: Reg) -> Option<u64> {
        if reg.is_zero() {
            return Some(0);
        }
        self.0[reg.index()]
    }

    fn set(&mut self, reg: Reg, value: Option<u64>) {
        if !reg.is_zero() {
            self.0[reg.index()] = value;
        }
    }

    /// Update the tracked values for one instruction.
    fn step(&mut self, addr: u64, inst: &Inst) {
        match inst {
            // The two ways an absolute address enters a register.
            Inst::Auipc { rd, imm } => self.set(*rd, Some(addr.wrapping_add(*imm as u64))),
            Inst::Lui { rd, imm } => self.set(*rd, Some(*imm as u64)),

            // `addi` completes the low half of an `auipc` pair.
            Inst::AluImm {
                op: AluOp::Add,
                rd,
                rs1,
                imm,
            } => {
                let value = self.get(*rs1).map(|v| v.wrapping_add(*imm as u64));
                self.set(*rd, value);
            }

            // `c.mv` decodes to `add rd, zero, rs2`, so a copy has to be
            // tracked or the pair is lost whenever LLVM shuffles registers.
            Inst::Alu {
                op: AluOp::Add,
                rd,
                rs1,
                rs2,
            } => {
                let value = match (self.get(*rs1), self.get(*rs2)) {
                    (Some(a), Some(b)) => Some(a.wrapping_add(b)),
                    _ => None,
                };
                self.set(*rd, value);
            }

            other => {
                if let Some(rd) = writes(other) {
                    self.set(rd, None);
                }
            }
        }
    }
}

/// The register an instruction writes, if any.
fn writes(inst: &Inst) -> Option<Reg> {
    match inst {
        Inst::Lui { rd, .. }
        | Inst::Auipc { rd, .. }
        | Inst::Jal { rd, .. }
        | Inst::Jalr { rd, .. }
        | Inst::Load { rd, .. }
        | Inst::Alu { rd, .. }
        | Inst::AluImm { rd, .. }
        | Inst::AluW { rd, .. }
        | Inst::AluImmW { rd, .. }
        | Inst::Mul { rd, .. }
        | Inst::MulW { rd, .. }
        | Inst::Amo { rd, .. }
        | Inst::LoadReserved { rd, .. }
        | Inst::StoreConditional { rd, .. } => Some(*rd),
        Inst::Branch { .. }
        | Inst::Store { .. }
        | Inst::Fence
        | Inst::Ecall
        | Inst::Ebreak
        | Inst::Unimp => None,
    }
}

/// The registers an instruction reads.
fn reads(inst: &Inst) -> Vec<Reg> {
    match inst {
        Inst::Lui { .. } | Inst::Auipc { .. } | Inst::Jal { .. } => vec![],
        Inst::Jalr { rs1, .. }
        | Inst::Load { rs1, .. }
        | Inst::AluImm { rs1, .. }
        | Inst::AluImmW { rs1, .. }
        | Inst::LoadReserved { rs1, .. } => vec![*rs1],
        Inst::Branch { rs1, rs2, .. }
        | Inst::Store { rs1, rs2, .. }
        | Inst::Alu { rs1, rs2, .. }
        | Inst::AluW { rs1, rs2, .. }
        | Inst::Mul { rs1, rs2, .. }
        | Inst::MulW { rs1, rs2, .. }
        | Inst::Amo { rs1, rs2, .. }
        | Inst::StoreConditional { rs1, rs2, .. } => vec![*rs1, *rs2],
        Inst::Fence | Inst::Ebreak | Inst::Unimp => vec![],
        // A host call may read any argument register.
        Inst::Ecall => (10..=17).map(Reg::new).collect(),
    }
}
