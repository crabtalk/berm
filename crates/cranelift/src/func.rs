//! Translating one RISC-V function into one CLIF function

use crate::{Analysis, Target, Trap, inst, offsets, params};
use anyhow::{Result, bail};
use cranelift::{
    codegen::ir::{FuncRef, SigRef},
    prelude::*,
};
use rv::{Function, Inst, Reg};
use std::collections::{BTreeMap, HashMap};

/// Everything the translator needs that lives outside the function.
pub struct Imports<'a> {
    /// Callee entry address to its imported reference.
    pub calls: &'a HashMap<u64, FuncRef>,

    /// Signature used for indirect calls between guest functions.
    pub indirect: SigRef,

    /// Signature of the host-call trampoline, `fn(*mut VmCtx)` in the
    /// platform's C convention.
    pub host: SigRef,

    /// `guest memory size - 1`, applied to every computed address.
    ///
    /// This is what confines the guest to its own address space, so it has to
    /// match the size the memory was actually mapped with. See
    /// [`crate::inst::address`].
    pub memory_mask: i64,
}

/// Translate `function` into `builder`'s function.
pub fn translate(
    function: &Function,
    analysis: &Analysis,
    imports: &Imports<'_>,
    builder: FunctionBuilder<'_>,
    frontend: isa::TargetFrontendConfig,
) -> Result<()> {
    let mut translator = Translator {
        function,
        analysis,
        imports,
        builder,
        regs: [Variable::new(0); 32],
        blocks: BTreeMap::new(),
        vmctx: Value::from_u32(0),
        memory: Value::from_u32(0),
        terminated: false,
    };

    translator.prologue()?;
    translator.body()?;
    translator.builder.seal_all_blocks();
    translator.builder.finalize(frontend);
    Ok(())
}

struct Translator<'a, 'b> {
    function: &'a Function,
    analysis: &'a Analysis,
    imports: &'a Imports<'a>,
    builder: FunctionBuilder<'b>,

    /// One CLIF variable per guest register. `x0` is never read from here.
    regs: [Variable; 32],

    /// CLIF block for each basic block leader.
    blocks: BTreeMap<u64, Block>,

    vmctx: Value,
    memory: Value,

    /// Whether the block being emitted already ended in a terminator.
    terminated: bool,
}

impl Translator<'_, '_> {
    /// Set up registers, the memory base, and the block map.
    fn prologue(&mut self) -> Result<()> {
        let entry = self.builder.create_block();
        self.builder.append_block_params_for_function_params(entry);
        self.builder.switch_to_block(entry);

        let args = self.builder.block_params(entry).to_vec();
        self.vmctx = args[params::VMCTX];
        self.memory =
            self.builder
                .ins()
                .load(types::I64, MemFlagsData::trusted(), self.vmctx, offsets::MEMORY);

        for slot in &mut self.regs {
            *slot = self.builder.declare_var(types::I64);
        }

        // Live-in registers come from the signature; everything else starts at
        // zero. See the crate docs for why a zeroed callee-saved register is
        // unobservable.
        let zero = self.builder.ins().iconst(types::I64, 0);
        for index in 0..32 {
            self.builder.def_var(self.regs[index], zero);
        }
        self.builder.def_var(self.regs[Reg::SP.index()], args[params::SP]);
        for arg in 0..params::ARGS {
            self.builder
                .def_var(self.regs[Reg::A0.index() + arg], args[params::A0 + arg]);
        }

        // `gp` and `tp` are program-wide and live in the VM context.
        if self.analysis.reads_globals {
            for reg in [Reg::GP, Reg::TP] {
                let value = self.builder.ins().load(
                    types::I64,
                    MemFlagsData::trusted(),
                    self.vmctx,
                    offsets::reg(reg.index()),
                );
                self.builder.def_var(self.regs[reg.index()], value);
            }
        }

        for &leader in &self.analysis.leaders {
            let block = self.builder.create_block();
            self.blocks.insert(leader, block);
        }

        let start = self.blocks[&self.function.range.start];
        self.builder.ins().jump(start, &[]);
        self.builder.seal_block(entry);

        // The entry block is now closed. Without this, `body` would treat it
        // as open and emit a second jump into a filled block.
        self.terminated = true;
        Ok(())
    }

    /// Emit every instruction, switching blocks at leaders.
    fn body(&mut self) -> Result<()> {
        for index in 0..self.function.code.len() {
            let (addr, inst) = self.function.code[index];

            if let Some(&block) = self.blocks.get(&addr) {
                // Fall through into the next block if the previous one did not
                // end in a terminator.
                if !self.terminated {
                    self.builder.ins().jump(block, &[]);
                }
                self.builder.switch_to_block(block);
                self.terminated = false;
            }

            if self.terminated {
                // Unreachable: an instruction after an unconditional transfer
                // that nothing branches to.
                continue;
            }

            let next = self
                .function
                .code
                .get(index + 1)
                .map(|(a, _)| *a)
                .unwrap_or(self.function.range.end);

            self.instruction(addr, inst, next)?;
        }

        // A function whose last block just runs off the end returns whatever
        // it has; this happens when the final instruction is not a terminator.
        if !self.terminated {
            self.emit_return();
        }
        Ok(())
    }

    /// Emit one instruction.
    fn instruction(&mut self, addr: u64, inst: Inst, next: u64) -> Result<()> {
        match inst {
            Inst::Lui { rd, imm } => {
                let value = self.builder.ins().iconst(types::I64, imm);
                self.rset(rd, value);
            }
            Inst::Auipc { rd, imm } => {
                let value = self
                    .builder
                    .ins()
                    .iconst(types::I64, addr.wrapping_add(imm as u64) as i64);
                self.rset(rd, value);
            }

            Inst::Branch { op, rs1, rs2, imm: _ } => {
                let lhs = self.rget(rs1);
                let rhs = self.rget(rs2);
                let cond = self.builder.ins().icmp(inst::condition(op), lhs, rhs);
                self.branch(addr, cond, next)?;
            }

            Inst::Jal { .. } | Inst::Jalr { .. } => self.transfer(addr, inst)?,

            Inst::Load { op, rd, rs1, imm } => {
                let base = self.rget(rs1);
                let value = inst::load(&mut self.builder, self.memory, op, base, imm, self.imports.memory_mask);
                self.rset(rd, value);
            }
            Inst::Store { op, rs1, rs2, imm } => {
                let base = self.rget(rs1);
                let value = self.rget(rs2);
                inst::store(
                    &mut self.builder,
                    self.memory,
                    op,
                    base,
                    imm,
                    value,
                    self.imports.memory_mask,
                );
            }

            Inst::Alu { op, rd, rs1, rs2 } => {
                let lhs = self.rget(rs1);
                let rhs = self.rget(rs2);
                let value = inst::alu(&mut self.builder, op, lhs, rhs, false);
                self.rset(rd, value);
            }
            Inst::AluImm { op, rd, rs1, imm } => {
                let lhs = self.rget(rs1);
                let rhs = self.builder.ins().iconst(types::I64, imm);
                let value = inst::alu(&mut self.builder, op, lhs, rhs, false);
                self.rset(rd, value);
            }
            Inst::AluW { op, rd, rs1, rs2 } => {
                let lhs = self.rget(rs1);
                let rhs = self.rget(rs2);
                let value = inst::alu(&mut self.builder, op, lhs, rhs, true);
                self.rset(rd, value);
            }
            Inst::AluImmW { op, rd, rs1, imm } => {
                let lhs = self.rget(rs1);
                let rhs = self.builder.ins().iconst(types::I64, imm);
                let value = inst::alu(&mut self.builder, op, lhs, rhs, true);
                self.rset(rd, value);
            }

            Inst::Mul { op, rd, rs1, rs2 } => {
                let lhs = self.rget(rs1);
                let rhs = self.rget(rs2);
                let value = inst::muldiv(&mut self.builder, op, lhs, rhs, false);
                self.rset(rd, value);
            }
            Inst::MulW { op, rd, rs1, rs2 } => {
                let lhs = self.rget(rs1);
                let rhs = self.rget(rs2);
                let value = inst::muldiv(&mut self.builder, op, lhs, rhs, true);
                self.rset(rd, value);
            }

            Inst::Amo { op, width, rd, rs1, rs2, .. } => {
                let addr_value = self.rget(rs1);
                let operand = self.rget(rs2);
                let value = inst::amo(
                    &mut self.builder,
                    self.memory,
                    op,
                    width,
                    addr_value,
                    operand,
                    self.imports.memory_mask,
                );
                self.rset(rd, value);
            }
            Inst::LoadReserved { width, rd, rs1, .. } => {
                let addr_value = self.rget(rs1);
                let value = inst::atomic_load(
                    &mut self.builder,
                    self.memory,
                    width,
                    addr_value,
                    self.imports.memory_mask,
                );
                self.rset(rd, value);
            }
            Inst::StoreConditional { width, rd, rs1, rs2, .. } => {
                let addr_value = self.rget(rs1);
                let value = self.rget(rs2);
                inst::atomic_store(
                    &mut self.builder,
                    self.memory,
                    width,
                    addr_value,
                    value,
                    self.imports.memory_mask,
                );
                // Single-threaded: the reservation can never be broken, so the
                // store always succeeds and reports zero.
                let ok = self.builder.ins().iconst(types::I64, 0);
                self.rset(rd, ok);
            }

            // No other thread can observe ordering in a single-threaded guest.
            Inst::Fence => {}

            Inst::Ecall => self.ecall(),
            Inst::Ebreak => {
                self.trap(Trap::Breakpoint);
                self.terminated = true;
            }
        }

        Ok(())
    }

    /// Emit a conditional branch.
    fn branch(&mut self, addr: u64, cond: Value, next: u64) -> Result<()> {
        let taken = match self.analysis.targets.get(&addr) {
            Some(Target::Local(dest)) => self.blocks[dest],
            Some(Target::Direct { addr: dest, .. }) => {
                // A branch out of the function: give the taken edge its own
                // block that performs the call and returns.
                let block = self.builder.create_block();
                let fallthrough = self.blocks[&next];
                self.builder.ins().brif(cond, block, &[], fallthrough, &[]);
                self.builder.switch_to_block(block);
                let dest = *dest;
                self.call(dest, true)?;
                self.terminated = true;
                return Ok(());
            }
            other => bail!("branch at {addr:#x} resolved to {other:?}"),
        };

        let fallthrough = self.blocks[&next];
        self.builder.ins().brif(cond, taken, &[], fallthrough, &[]);
        self.terminated = true;
        Ok(())
    }

    /// Emit a jump, call, or return.
    fn transfer(&mut self, addr: u64, inst: Inst) -> Result<()> {
        let target = match self.analysis.targets.get(&addr) {
            Some(target) => *target,
            None => bail!("unresolved control transfer at {addr:#x}"),
        };

        match target {
            Target::Local(dest) => {
                let block = self.blocks[&dest];
                self.builder.ins().jump(block, &[]);
                self.terminated = true;
            }
            Target::Return => self.emit_return(),
            Target::Direct { addr: dest, tail } => self.call(dest, tail)?,
            Target::Indirect { tail } => {
                let Inst::Jalr { rs1, imm, .. } = inst else {
                    bail!("indirect transfer at {addr:#x} is not a jalr");
                };
                let base = self.rget(rs1);
                let computed = self.builder.ins().iadd_imm_s(base, imm);
                // The architecture clears the low bit of a jalr target.
                let dest = self.builder.ins().band_imm_s(computed, !1);
                self.call_indirect(dest, tail)?;
            }
        }

        Ok(())
    }

    /// Emit a direct call, returning immediately if it was a tail call.
    fn call(&mut self, dest: u64, tail: bool) -> Result<()> {
        let Some(&callee) = self.imports.calls.get(&dest) else {
            bail!("callee {dest:#x} was not imported");
        };

        let args = self.call_args();
        let inst = self.builder.ins().call(callee, &args);
        let results = self.builder.inst_results(inst).to_vec();
        self.apply_results(&results);

        if tail {
            self.emit_return();
        }
        Ok(())
    }

    /// Emit an indirect call through the dispatch table.
    ///
    /// The table maps a guest code address to the entry point compiled for it.
    /// A target that is not a known function entry has no slot and traps,
    /// which is the check that keeps a corrupted function pointer from
    /// becoming an arbitrary jump.
    fn call_indirect(&mut self, dest: Value, tail: bool) -> Result<()> {
        let text_base = self.builder.ins().load(
            types::I64,
            MemFlagsData::trusted(),
            self.vmctx,
            offsets::TEXT_BASE,
        );
        let table = self.builder.ins().load(
            types::I64,
            MemFlagsData::trusted(),
            self.vmctx,
            offsets::DISPATCH,
        );
        let len = self.builder.ins().load(
            types::I64,
            MemFlagsData::trusted(),
            self.vmctx,
            offsets::DISPATCH_LEN,
        );

        // Instructions are two-byte aligned, so the table has one slot per
        // possible entry address rather than per byte.
        let offset = self.builder.ins().isub(dest, text_base);
        let index = self.builder.ins().ushr_imm_u(offset, 1);

        let in_range = self
            .builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, index, len);
        let ok = self.builder.create_block();
        let bad = self.builder.create_block();
        self.builder.ins().brif(in_range, ok, &[], bad, &[]);

        self.builder.switch_to_block(bad);
        self.trap(Trap::BadIndirectTarget);

        self.builder.switch_to_block(ok);
        let slot = self.builder.ins().imul_imm_u(index, 8);
        let address = self.builder.ins().iadd(table, slot);
        let callee = self
            .builder
            .ins()
            .load(types::I64, MemFlagsData::trusted(), address, 0);

        // An empty slot means the address is inside .text but is not an entry
        // point any function pointer could legitimately hold.
        let known = self.builder.ins().icmp_imm_s(IntCC::NotEqual, callee, 0);
        let go = self.builder.create_block();
        let unknown = self.builder.create_block();
        self.builder.ins().brif(known, go, &[], unknown, &[]);

        self.builder.switch_to_block(unknown);
        self.trap(Trap::BadIndirectTarget);

        self.builder.switch_to_block(go);
        let args = self.call_args();
        let inst = self
            .builder
            .ins()
            .call_indirect(self.imports.indirect, callee, &args);
        let results = self.builder.inst_results(inst).to_vec();
        self.apply_results(&results);

        if tail {
            self.emit_return();
        }
        Ok(())
    }

    /// Call into the host.
    ///
    /// The handler reads arguments and writes results through the VM context,
    /// so the argument registers are flushed before and reloaded after. It
    /// returns a status: anything non-zero means the host refused, and the
    /// guest must not keep running.
    fn ecall(&mut self) {
        self.flush(Reg::A0.index()..=Reg::A7.index());
        self.flush_globals();

        let callee = self.builder.ins().load(
            types::I64,
            MemFlagsData::trusted(),
            self.vmctx,
            offsets::HOST_CALL,
        );
        let call = self
            .builder
            .ins()
            .call_indirect(self.imports.host, callee, &[self.vmctx]);
        let status = self.builder.inst_results(call)[0];

        let failed = self.builder.create_block();
        let resume = self.builder.create_block();
        let ok = self.builder.ins().icmp_imm_s(IntCC::Equal, status, 0);
        self.builder.ins().brif(ok, resume, &[], failed, &[]);

        // The handler has already recorded why it failed.
        self.builder.switch_to_block(failed);
        self.ret();

        self.builder.switch_to_block(resume);
        self.reload(Reg::A0.index()..=Reg::A7.index());
        if self.analysis.reads_globals {
            self.reload(Reg::GP.index()..=Reg::TP.index());
        }
    }

    /// Store a range of registers into the VM context.
    fn flush(&mut self, range: std::ops::RangeInclusive<usize>) {
        for index in range {
            let value = self.builder.use_var(self.regs[index]);
            self.builder.ins().store(
                MemFlagsData::trusted(),
                value,
                self.vmctx,
                offsets::reg(index),
            );
        }
    }

    /// Load a range of registers back from the VM context.
    fn reload(&mut self, range: std::ops::RangeInclusive<usize>) {
        for index in range {
            let value = self.builder.ins().load(
                types::I64,
                MemFlagsData::trusted(),
                self.vmctx,
                offsets::reg(index),
            );
            self.builder.def_var(self.regs[index], value);
        }
    }

    /// Write `gp` and `tp` back if this function modifies them.
    fn flush_globals(&mut self) {
        if self.analysis.writes_globals {
            self.flush(Reg::GP.index()..=Reg::TP.index());
        }
    }

    /// Arguments for a guest-to-guest call.
    fn call_args(&mut self) -> Vec<Value> {
        let mut args = Vec::with_capacity(params::COUNT);
        args.push(self.vmctx);
        args.push(self.rget(Reg::SP));
        for arg in 0..params::ARGS {
            args.push(self.rget(Reg::new(Reg::A0.index() as u8 + arg as u8)));
        }
        args
    }

    /// Adopt the registers a callee returned.
    fn apply_results(&mut self, results: &[Value]) {
        self.rset(Reg::SP, results[0]);
        self.rset(Reg::A0, results[1]);
        self.rset(Reg::A1, results[2]);
    }

    /// Emit a return of the ABI-live registers.
    ///
    /// This closes the block currently being built but says nothing about the
    /// block the *instruction stream* is in, so it does not touch
    /// [`Self::terminated`]. Callers that are ending a guest instruction set
    /// the flag themselves; callers filling a side block (a trap path, say)
    /// must not.
    fn ret(&mut self) {
        self.flush_globals();
        let sp = self.rget(Reg::SP);
        let a0 = self.rget(Reg::A0);
        let a1 = self.rget(Reg::A1);
        self.builder.ins().return_(&[sp, a0, a1]);
    }

    /// Return, ending the current guest instruction.
    fn emit_return(&mut self) {
        self.ret();
        self.terminated = true;
    }

    /// Record a trap code and unwind to the host.
    ///
    /// Used to fill side blocks, so it leaves [`Self::terminated`] alone.
    fn trap(&mut self, trap: Trap) {
        let code = self.builder.ins().iconst(types::I64, trap as i64);
        self.builder
            .ins()
            .store(MemFlagsData::trusted(), code, self.vmctx, offsets::TRAP);
        self.ret();
    }

    /// Read a guest register. `x0` folds to a constant.
    fn rget(&mut self, reg: Reg) -> Value {
        if reg.is_zero() {
            return self.builder.ins().iconst(types::I64, 0);
        }
        self.builder.use_var(self.regs[reg.index()])
    }

    /// Write a guest register. Writes to `x0` are discarded.
    fn rset(&mut self, reg: Reg, value: Value) {
        if reg.is_zero() {
            return;
        }
        self.builder.def_var(self.regs[reg.index()], value);
    }
}
