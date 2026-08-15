//! Lowering individual instructions to CLIF

use cranelift::prelude::*;
use rv::{AluOp, AmoOp, Cond, LoadOp, MulOp, StoreOp, Width};

/// The CLIF comparison for a RISC-V branch condition.
pub fn condition(op: Cond) -> IntCC {
    match op {
        Cond::Eq => IntCC::Equal,
        Cond::Ne => IntCC::NotEqual,
        Cond::Lt => IntCC::SignedLessThan,
        Cond::Ge => IntCC::SignedGreaterThanOrEqual,
        Cond::LtU => IntCC::UnsignedLessThan,
        Cond::GeU => IntCC::UnsignedGreaterThanOrEqual,
    }
}

/// Translate a guest address into a host one.
///
/// The mask is the sandbox. Guest addresses are 64-bit values a program is free
/// to compute arbitrarily; `and`-ing with `size - 1` keeps every access inside
/// the reserved window, where anything not committed is a guard page. Without
/// it a guest could compute an address past the reservation and read host
/// memory.
///
/// This confines rather than bounds-checks: an out-of-range address wraps and
/// may land on a committed page instead of faulting. That is enough for a
/// sandbox -- the guest can only reach its own memory -- but it is not a
/// precise bounds check, and it is why the size must be a power of two.
pub fn address(b: &mut FunctionBuilder, memory: Value, base: Value, imm: i64, mask: i64) -> Value {
    let addr = b.ins().iadd_imm_s(base, imm);
    let masked = b.ins().band_imm_u(addr, mask);
    b.ins().iadd(memory, masked)
}

/// Flags for guest accesses.
///
/// Deliberately not `MemFlagsData::trusted()`: these accesses *can* trap, which is
/// how bounds checking works, and RISC-V permits unaligned access.
fn guest_access() -> MemFlagsData {
    MemFlagsData::new()
}

/// Emit a load, extending to 64 bits as the opcode requires.
pub fn load(
    b: &mut FunctionBuilder,
    memory: Value,
    op: LoadOp,
    base: Value,
    imm: i64,
    mask: i64,
) -> Value {
    let addr = address(b, memory, base, imm, mask);
    let flags = guest_access();

    match op {
        LoadOp::B => {
            let v = b.ins().load(types::I8, flags, addr, 0);
            b.ins().sextend(types::I64, v)
        }
        LoadOp::H => {
            let v = b.ins().load(types::I16, flags, addr, 0);
            b.ins().sextend(types::I64, v)
        }
        LoadOp::W => {
            let v = b.ins().load(types::I32, flags, addr, 0);
            b.ins().sextend(types::I64, v)
        }
        LoadOp::D => b.ins().load(types::I64, flags, addr, 0),
        LoadOp::Bu => {
            let v = b.ins().load(types::I8, flags, addr, 0);
            b.ins().uextend(types::I64, v)
        }
        LoadOp::Hu => {
            let v = b.ins().load(types::I16, flags, addr, 0);
            b.ins().uextend(types::I64, v)
        }
        LoadOp::Wu => {
            let v = b.ins().load(types::I32, flags, addr, 0);
            b.ins().uextend(types::I64, v)
        }
    }
}

/// Emit a store, truncating to the opcode's width.
pub fn store(
    b: &mut FunctionBuilder,
    memory: Value,
    op: StoreOp,
    base: Value,
    imm: i64,
    value: Value,
    mask: i64,
) {
    let addr = address(b, memory, base, imm, mask);
    let flags = guest_access();

    let narrowed = match op {
        StoreOp::B => b.ins().ireduce(types::I8, value),
        StoreOp::H => b.ins().ireduce(types::I16, value),
        StoreOp::W => b.ins().ireduce(types::I32, value),
        StoreOp::D => value,
    };
    b.ins().store(flags, narrowed, addr, 0);
}

/// Emit a base integer ALU operation.
///
/// `word` selects the `*w` forms, which compute on the low 32 bits and
/// sign-extend the result back to 64.
pub fn alu(b: &mut FunctionBuilder, op: AluOp, lhs: Value, rhs: Value, word: bool) -> Value {
    if word {
        let lhs = b.ins().ireduce(types::I32, lhs);
        let rhs = b.ins().ireduce(types::I32, rhs);
        let result = alu_typed(b, op, lhs, rhs, types::I32);
        return b.ins().sextend(types::I64, result);
    }
    alu_typed(b, op, lhs, rhs, types::I64)
}

fn alu_typed(
    b: &mut FunctionBuilder,
    op: AluOp,
    lhs: Value,
    rhs: Value,
    ty: types::Type,
) -> Value {
    match op {
        AluOp::Add => b.ins().iadd(lhs, rhs),
        AluOp::Sub => b.ins().isub(lhs, rhs),
        // RISC-V masks the shift amount to the operand width, which is exactly
        // what CLIF's shifts already do.
        AluOp::Sll => b.ins().ishl(lhs, rhs),
        AluOp::Srl => b.ins().ushr(lhs, rhs),
        AluOp::Sra => b.ins().sshr(lhs, rhs),
        AluOp::Xor => b.ins().bxor(lhs, rhs),
        AluOp::Or => b.ins().bor(lhs, rhs),
        AluOp::And => b.ins().band(lhs, rhs),
        AluOp::Slt => {
            let flag = b.ins().icmp(IntCC::SignedLessThan, lhs, rhs);
            b.ins().uextend(ty, flag)
        }
        AluOp::SltU => {
            let flag = b.ins().icmp(IntCC::UnsignedLessThan, lhs, rhs);
            b.ins().uextend(ty, flag)
        }
    }
}

/// Emit an M-extension operation.
///
/// RISC-V defines division by zero and signed overflow to produce specific
/// values rather than trapping, so the divisor is sanitised first and the
/// special results are selected afterwards. CLIF's `sdiv`/`udiv` trap on those
/// inputs, so this cannot be left to the hardware.
pub fn muldiv(b: &mut FunctionBuilder, op: MulOp, lhs: Value, rhs: Value, word: bool) -> Value {
    if word {
        let lhs = b.ins().ireduce(types::I32, lhs);
        let rhs = b.ins().ireduce(types::I32, rhs);
        let result = muldiv_typed(b, op, lhs, rhs, types::I32);
        return b.ins().sextend(types::I64, result);
    }
    muldiv_typed(b, op, lhs, rhs, types::I64)
}

fn muldiv_typed(
    b: &mut FunctionBuilder,
    op: MulOp,
    lhs: Value,
    rhs: Value,
    ty: types::Type,
) -> Value {
    let min = if ty == types::I32 {
        i32::MIN as i64
    } else {
        i64::MIN
    };

    match op {
        MulOp::Mul => b.ins().imul(lhs, rhs),
        MulOp::MulH => b.ins().smulhi(lhs, rhs),
        MulOp::MulHU => b.ins().umulhi(lhs, rhs),
        // mulhsu has no CLIF equivalent. Treating the signed operand as
        // unsigned differs from the true product by 2^N * rhs exactly when the
        // operand is negative, so correcting the unsigned high half by `rhs`
        // in that case gives the signed-by-unsigned result.
        MulOp::MulHSU => {
            let high = b.ins().umulhi(lhs, rhs);
            let negative = b.ins().icmp_imm_s(IntCC::SignedLessThan, lhs, 0);
            let zero = b.ins().iconst(ty, 0);
            let correction = b.ins().select(negative, rhs, zero);
            b.ins().isub(high, correction)
        }

        MulOp::Div => {
            let (safe, by_zero, overflow) = sanitise(b, lhs, rhs, ty, min);
            let quotient = b.ins().sdiv(lhs, safe);
            let min_value = b.ins().iconst(ty, min);
            let quotient = b.ins().select(overflow, min_value, quotient);
            let all_ones = b.ins().iconst(ty, -1);
            b.ins().select(by_zero, all_ones, quotient)
        }
        MulOp::Rem => {
            let (safe, by_zero, overflow) = sanitise(b, lhs, rhs, ty, min);
            let remainder = b.ins().srem(lhs, safe);
            let zero = b.ins().iconst(ty, 0);
            let remainder = b.ins().select(overflow, zero, remainder);
            b.ins().select(by_zero, lhs, remainder)
        }
        MulOp::DivU => {
            let by_zero = b.ins().icmp_imm_s(IntCC::Equal, rhs, 0);
            let one = b.ins().iconst(ty, 1);
            let safe = b.ins().select(by_zero, one, rhs);
            let quotient = b.ins().udiv(lhs, safe);
            let all_ones = b.ins().iconst(ty, -1);
            b.ins().select(by_zero, all_ones, quotient)
        }
        MulOp::RemU => {
            let by_zero = b.ins().icmp_imm_s(IntCC::Equal, rhs, 0);
            let one = b.ins().iconst(ty, 1);
            let safe = b.ins().select(by_zero, one, rhs);
            let remainder = b.ins().urem(lhs, safe);
            b.ins().select(by_zero, lhs, remainder)
        }
    }
}

/// Replace a divisor that would trap with 1, reporting why.
fn sanitise(
    b: &mut FunctionBuilder,
    lhs: Value,
    rhs: Value,
    ty: types::Type,
    min: i64,
) -> (Value, Value, Value) {
    let by_zero = b.ins().icmp_imm_s(IntCC::Equal, rhs, 0);
    let is_min = b.ins().icmp_imm_s(IntCC::Equal, lhs, min);
    let is_neg_one = b.ins().icmp_imm_s(IntCC::Equal, rhs, -1);
    let overflow = b.ins().band(is_min, is_neg_one);

    let unsafe_divisor = b.ins().bor(by_zero, overflow);
    let one = b.ins().iconst(ty, 1);
    let safe = b.ins().select(unsafe_divisor, one, rhs);

    (safe, by_zero, overflow)
}

/// The CLIF type for an atomic width.
fn atomic_type(width: Width) -> types::Type {
    match width {
        Width::W => types::I32,
        Width::D => types::I64,
    }
}

/// Emit an atomic read-modify-write.
///
/// The guest is single-threaded, so these lower to a plain load, compute and
/// store. Nothing can observe the sequence as non-atomic.
pub fn amo(
    b: &mut FunctionBuilder,
    memory: Value,
    op: AmoOp,
    width: Width,
    base: Value,
    operand: Value,
    mask: i64,
) -> Value {
    let ty = atomic_type(width);
    let addr = address(b, memory, base, 0, mask);
    let flags = guest_access();

    let old = b.ins().load(ty, flags, addr, 0);
    let operand = if ty == types::I32 {
        b.ins().ireduce(types::I32, operand)
    } else {
        operand
    };

    let new = match op {
        AmoOp::Swap => operand,
        AmoOp::Add => b.ins().iadd(old, operand),
        AmoOp::Xor => b.ins().bxor(old, operand),
        AmoOp::And => b.ins().band(old, operand),
        AmoOp::Or => b.ins().bor(old, operand),
        AmoOp::Min => b.ins().smin(old, operand),
        AmoOp::Max => b.ins().smax(old, operand),
        AmoOp::MinU => b.ins().umin(old, operand),
        AmoOp::MaxU => b.ins().umax(old, operand),
    };
    b.ins().store(flags, new, addr, 0);

    // The architecture sign-extends the 32-bit forms' result.
    if ty == types::I32 {
        b.ins().sextend(types::I64, old)
    } else {
        old
    }
}

/// Emit `lr.{w,d}`.
pub fn atomic_load(
    b: &mut FunctionBuilder,
    memory: Value,
    width: Width,
    base: Value,
    mask: i64,
) -> Value {
    let ty = atomic_type(width);
    let addr = address(b, memory, base, 0, mask);
    let value = b.ins().load(ty, guest_access(), addr, 0);
    if ty == types::I32 {
        b.ins().sextend(types::I64, value)
    } else {
        value
    }
}

/// Emit the store half of `sc.{w,d}`.
pub fn atomic_store(
    b: &mut FunctionBuilder,
    memory: Value,
    width: Width,
    base: Value,
    value: Value,
    mask: i64,
) {
    let ty = atomic_type(width);
    let addr = address(b, memory, base, 0, mask);
    let narrowed = if ty == types::I32 {
        b.ins().ireduce(types::I32, value)
    } else {
        value
    };
    b.ins().store(guest_access(), narrowed, addr, 0);
}
