//! Individual instruction encodings, hand-checked against the fixtures.
//!
//! These pin down specific bit patterns. The whole-`.text` differential
//! against LLVM lives in `decode.rs`.

use rvtime_core::{AluOp, AmoOp, Cond, Inst, LoadOp, MulOp, Ordering, Reg, StoreOp, Width, decode};

fn one(bytes: &[u8]) -> Inst {
    let (inst, len) = decode(bytes).expect("decodes");
    assert_eq!(len, bytes.len(), "consumed the whole input");
    inst
}

/// Encodings lifted from the disassembly of the `basic` fixture.
#[test]
fn fixture_encodings() {
    // 8082  ret  == jalr zero, 0(ra)
    assert_eq!(
        one(&[0x82, 0x80]),
        Inst::Jalr { rd: Reg::ZERO, rs1: Reg::RA, imm: 0 }
    );
    // 952e  add a0, a0, a1
    assert_eq!(
        one(&[0x2e, 0x95]),
        Inst::Alu { op: AluOp::Add, rd: Reg::A0, rs1: Reg::A0, rs2: Reg::A1 }
    );
    // 8d0d  sub a0, a0, a1
    assert_eq!(
        one(&[0x0d, 0x8d]),
        Inst::Alu { op: AluOp::Sub, rd: Reg::A0, rs1: Reg::A0, rs2: Reg::A1 }
    );
    // 1101  addi sp, sp, -0x20
    assert_eq!(
        one(&[0x01, 0x11]),
        Inst::AluImm { op: AluOp::Add, rd: Reg::SP, rs1: Reg::SP, imm: -32 }
    );
    // ec06  sd ra, 0x18(sp)
    assert_eq!(
        one(&[0x06, 0xec]),
        Inst::Store { op: StoreOp::D, rs1: Reg::SP, rs2: Reg::RA, imm: 0x18 }
    );
    // 4505  li a0, 0x1  == addi a0, zero, 1
    assert_eq!(
        one(&[0x05, 0x45]),
        Inst::AluImm { op: AluOp::Add, rd: Reg::A0, rs1: Reg::ZERO, imm: 1 }
    );
    // 8782  jr a5  == jalr zero, 0(a5)
    assert_eq!(
        one(&[0x82, 0x87]),
        Inst::Jalr { rd: Reg::ZERO, rs1: Reg::A5, imm: 0 }
    );
    // 02a58533  mul a0, a1, a0
    assert_eq!(
        one(&[0x33, 0x85, 0xa5, 0x02]),
        Inst::Mul { op: MulOp::Mul, rd: Reg::A0, rs1: Reg::A1, rs2: Reg::A0 }
    );
    // 00000097  auipc ra, 0x0
    assert_eq!(one(&[0x97, 0x00, 0x00, 0x00]), Inst::Auipc { rd: Reg::RA, imm: 0 });
    // 04a080e7  jalr 0x4a(ra)
    assert_eq!(
        one(&[0xe7, 0x80, 0xa0, 0x04]),
        Inst::Jalr { rd: Reg::RA, rs1: Reg::RA, imm: 0x4a }
    );
    // 06a5b52f  amoadd.d.aqrl a0, a0, (a1)
    assert_eq!(
        one(&[0x2f, 0xb5, 0xa5, 0x06]),
        Inst::Amo {
            op: AmoOp::Add,
            width: Width::D,
            rd: Reg::A0,
            rs1: Reg::A1,
            rs2: Reg::A0,
            ord: Ordering { acquire: true, release: true },
        }
    );
    // 611c  ld a5, 0x0(a0)
    assert_eq!(
        one(&[0x1c, 0x61]),
        Inst::Load { op: LoadOp::D, rd: Reg::A5, rs1: Reg::A0, imm: 0 }
    );
    // fff58613  addi a2, a1, -0x1
    assert_eq!(
        one(&[0x13, 0x86, 0xf5, 0xff]),
        Inst::AluImm { op: AluOp::Add, rd: Reg::A2, rs1: Reg::A1, imm: -1 }
    );
    // 02069713  slli a4, a3, 0x20
    assert_eq!(
        one(&[0x13, 0x97, 0x06, 0x02]),
        Inst::AluImm { op: AluOp::Sll, rd: Reg::A4, rs1: Reg::A3, imm: 0x20 }
    );
    // 9af9  andi a3, a3, -0x2
    assert_eq!(
        one(&[0xf9, 0x9a]),
        Inst::AluImm { op: AluOp::And, rd: Reg::A3, rs1: Reg::A3, imm: -2 }
    );
    // c911  beqz a0, +0x14
    assert_eq!(
        one(&[0x11, 0xc9]),
        Inst::Branch { op: Cond::Eq, rs1: Reg::A0, rs2: Reg::ZERO, imm: 0x14 }
    );
    // a001  j 0x111f0 -- a branch to itself, the fixture's `loop {}`
    assert_eq!(one(&[0x01, 0xa0]), Inst::Jal { rd: Reg::ZERO, imm: 0 });
    // aaaab6b7  lui a3, 0xaaaab
    assert_eq!(
        one(&[0xb7, 0xb6, 0xaa, 0xaa]),
        Inst::Lui { rd: Reg::A3, imm: 0xaaaa_b000u32 as i32 as i64 }
    );
}

#[test]
fn length_follows_the_encoding() {
    assert_eq!(decode(&[0x82, 0x80]).unwrap().1, 2);
    assert_eq!(decode(&[0x97, 0x00, 0x00, 0x00]).unwrap().1, 4);
}

#[test]
fn ecall_and_ebreak() {
    assert_eq!(one(&[0x73, 0x00, 0x00, 0x00]), Inst::Ecall);
    assert_eq!(one(&[0x73, 0x00, 0x10, 0x00]), Inst::Ebreak);
    // 9002  c.ebreak
    assert_eq!(one(&[0x02, 0x90]), Inst::Ebreak);
}

#[test]
fn rejects_illegal_and_truncated_input() {
    assert!(decode(&[]).is_err());
    assert!(decode(&[0x97]).is_err());
    // A 32-bit opcode with only two bytes available.
    assert!(decode(&[0x97, 0x00]).is_err());
}

#[test]
fn terminators() {
    assert!(one(&[0x82, 0x80]).is_terminator());
    assert!(one(&[0x01, 0xa0]).is_terminator());
    assert!(one(&[0x11, 0xc9]).is_terminator());
    assert!(!one(&[0x2e, 0x95]).is_terminator());
}

#[test]
fn unimp_decodes_rather_than_failing() {
    // The all-zero halfword is a *defined* illegal instruction: the ISA
    // guarantees it traps, and compilers emit it after a diverging call. A
    // decoder that rejected it would fail to load any guest with a panic path.
    assert_eq!(one(&[0x00, 0x00]), Inst::Unimp);
    assert!(one(&[0x00, 0x00]).is_terminator());
}
