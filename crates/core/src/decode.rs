//! RV64IMAC instruction decoder
//!
//! Compressed (C) encodings expand into the same canonical [`Inst`] variants as
//! their 32-bit equivalents, so nothing downstream needs to know the extension
//! exists. The returned length is what advances the PC.

use crate::{AluOp, AmoOp, Cond, Inst, LoadOp, MulOp, Ordering, Reg, StoreOp, Width};
use anyhow::{Result, bail};

/// Decode one instruction from the front of `bytes`.
///
/// Returns the instruction and its encoded length in bytes (2 or 4).
pub fn decode(bytes: &[u8]) -> Result<(Inst, usize)> {
    if bytes.len() < 2 {
        bail!("truncated instruction: {} byte(s) available", bytes.len());
    }

    let half = u16::from_le_bytes([bytes[0], bytes[1]]);
    if half == 0 {
        return Ok((Inst::Unimp, 2));
    }
    if half & 0x3 != 0x3 {
        return Ok((compressed(half)?, 2));
    }

    if bytes.len() < 4 {
        bail!(
            "truncated 32-bit instruction: {} byte(s) available",
            bytes.len()
        );
    }
    let word = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if word & 0x1f == 0x1f {
        bail!("instruction longer than 32 bits is not supported: {word:#010x}");
    }
    Ok((uncompressed(word)?, 4))
}

/// Sign-extend the low `bits` of `value`.
const fn sext(value: u64, bits: u32) -> i64 {
    let shift = 64 - bits;
    ((value << shift) as i64) >> shift
}

// -- 32-bit encodings ------------------------------------------------------

fn uncompressed(w: u32) -> Result<Inst> {
    let rd = Reg::new((w >> 7) as u8);
    let rs1 = Reg::new((w >> 15) as u8);
    let rs2 = Reg::new((w >> 20) as u8);
    let funct3 = (w >> 12) & 0x7;
    let funct7 = (w >> 25) & 0x7f;

    // I-type immediate.
    let imm_i = sext((w >> 20) as u64, 12);
    // S-type immediate: imm[11:5] in funct7, imm[4:0] in the rd slot.
    let imm_s = sext(((funct7 << 5) | ((w >> 7) & 0x1f)) as u64, 12);
    // B-type immediate, scrambled across the word and always even.
    let imm_b = sext(
        ((((w >> 31) & 1) << 12)
            | (((w >> 7) & 1) << 11)
            | (((w >> 25) & 0x3f) << 5)
            | (((w >> 8) & 0xf) << 1)) as u64,
        13,
    );
    // U-type immediate, already shifted into place.
    let imm_u = (w & 0xffff_f000) as i32 as i64;
    // J-type immediate.
    let imm_j = sext(
        ((((w >> 31) & 1) << 20)
            | (((w >> 12) & 0xff) << 12)
            | (((w >> 20) & 1) << 11)
            | (((w >> 21) & 0x3ff) << 1)) as u64,
        21,
    );

    let inst = match w & 0x7f {
        0b0110111 => Inst::Lui { rd, imm: imm_u },
        0b0010111 => Inst::Auipc { rd, imm: imm_u },
        0b1101111 => Inst::Jal { rd, imm: imm_j },
        0b1100111 if funct3 == 0 => Inst::Jalr {
            rd,
            rs1,
            imm: imm_i,
        },

        0b1100011 => {
            let op = match funct3 {
                0b000 => Cond::Eq,
                0b001 => Cond::Ne,
                0b100 => Cond::Lt,
                0b101 => Cond::Ge,
                0b110 => Cond::LtU,
                0b111 => Cond::GeU,
                _ => bail!("reserved branch funct3 {funct3:#05b}: {w:#010x}"),
            };
            Inst::Branch {
                op,
                rs1,
                rs2,
                imm: imm_b,
            }
        }

        0b0000011 => {
            let op = match funct3 {
                0b000 => LoadOp::B,
                0b001 => LoadOp::H,
                0b010 => LoadOp::W,
                0b011 => LoadOp::D,
                0b100 => LoadOp::Bu,
                0b101 => LoadOp::Hu,
                0b110 => LoadOp::Wu,
                _ => bail!("reserved load funct3 {funct3:#05b}: {w:#010x}"),
            };
            Inst::Load {
                op,
                rd,
                rs1,
                imm: imm_i,
            }
        }

        0b0100011 => {
            let op = match funct3 {
                0b000 => StoreOp::B,
                0b001 => StoreOp::H,
                0b010 => StoreOp::W,
                0b011 => StoreOp::D,
                _ => bail!("reserved store funct3 {funct3:#05b}: {w:#010x}"),
            };
            Inst::Store {
                op,
                rs1,
                rs2,
                imm: imm_s,
            }
        }

        // OP-IMM: shifts take a 6-bit shamt and steal funct7's top bits.
        0b0010011 => match funct3 {
            0b001 => Inst::AluImm {
                op: AluOp::Sll,
                rd,
                rs1,
                imm: ((w >> 20) & 0x3f) as i64,
            },
            0b101 => {
                let op = if funct7 & 0b010_0000 != 0 {
                    AluOp::Sra
                } else {
                    AluOp::Srl
                };
                Inst::AluImm {
                    op,
                    rd,
                    rs1,
                    imm: ((w >> 20) & 0x3f) as i64,
                }
            }
            _ => {
                let op = match funct3 {
                    0b000 => AluOp::Add,
                    0b010 => AluOp::Slt,
                    0b011 => AluOp::SltU,
                    0b100 => AluOp::Xor,
                    0b110 => AluOp::Or,
                    0b111 => AluOp::And,
                    _ => unreachable!("shift funct3 handled above"),
                };
                Inst::AluImm {
                    op,
                    rd,
                    rs1,
                    imm: imm_i,
                }
            }
        },

        0b0110011 if funct7 == 0b000_0001 => {
            let op = match funct3 {
                0b000 => MulOp::Mul,
                0b001 => MulOp::MulH,
                0b010 => MulOp::MulHSU,
                0b011 => MulOp::MulHU,
                0b100 => MulOp::Div,
                0b101 => MulOp::DivU,
                0b110 => MulOp::Rem,
                0b111 => MulOp::RemU,
                _ => unreachable!("funct3 is 3 bits"),
            };
            Inst::Mul { op, rd, rs1, rs2 }
        }

        0b0110011 => {
            let alt = funct7 & 0b010_0000 != 0;
            let op = match (funct3, alt) {
                (0b000, false) => AluOp::Add,
                (0b000, true) => AluOp::Sub,
                (0b001, false) => AluOp::Sll,
                (0b010, false) => AluOp::Slt,
                (0b011, false) => AluOp::SltU,
                (0b100, false) => AluOp::Xor,
                (0b101, false) => AluOp::Srl,
                (0b101, true) => AluOp::Sra,
                (0b110, false) => AluOp::Or,
                (0b111, false) => AluOp::And,
                _ => bail!("reserved OP encoding: {w:#010x}"),
            };
            Inst::Alu { op, rd, rs1, rs2 }
        }

        // OP-IMM-32: word-width shifts use a 5-bit shamt.
        0b0011011 => match funct3 {
            0b000 => Inst::AluImmW {
                op: AluOp::Add,
                rd,
                rs1,
                imm: imm_i,
            },
            0b001 => Inst::AluImmW {
                op: AluOp::Sll,
                rd,
                rs1,
                imm: ((w >> 20) & 0x1f) as i64,
            },
            0b101 => {
                let op = if funct7 & 0b010_0000 != 0 {
                    AluOp::Sra
                } else {
                    AluOp::Srl
                };
                Inst::AluImmW {
                    op,
                    rd,
                    rs1,
                    imm: ((w >> 20) & 0x1f) as i64,
                }
            }
            _ => bail!("reserved OP-IMM-32 funct3 {funct3:#05b}: {w:#010x}"),
        },

        0b0111011 if funct7 == 0b000_0001 => {
            let op = match funct3 {
                0b000 => MulOp::Mul,
                0b100 => MulOp::Div,
                0b101 => MulOp::DivU,
                0b110 => MulOp::Rem,
                0b111 => MulOp::RemU,
                _ => bail!("reserved OP-32 M encoding: {w:#010x}"),
            };
            Inst::MulW { op, rd, rs1, rs2 }
        }

        0b0111011 => {
            let alt = funct7 & 0b010_0000 != 0;
            let op = match (funct3, alt) {
                (0b000, false) => AluOp::Add,
                (0b000, true) => AluOp::Sub,
                (0b001, false) => AluOp::Sll,
                (0b101, false) => AluOp::Srl,
                (0b101, true) => AluOp::Sra,
                _ => bail!("reserved OP-32 encoding: {w:#010x}"),
            };
            Inst::AluW { op, rd, rs1, rs2 }
        }

        // FENCE and FENCE.I are both no-ops for a single-threaded guest.
        0b0001111 => Inst::Fence,

        0b1110011 if funct3 == 0 => match w >> 20 {
            0 => Inst::Ecall,
            1 => Inst::Ebreak,
            other => bail!("unsupported SYSTEM immediate {other:#x}: {w:#010x}"),
        },

        0b0101111 => return atomic(w, rd, rs1, rs2, funct3),

        opcode => bail!("unsupported opcode {opcode:#09b}: {w:#010x}"),
    };

    Ok(inst)
}

fn atomic(w: u32, rd: Reg, rs1: Reg, rs2: Reg, funct3: u32) -> Result<Inst> {
    let width = match funct3 {
        0b010 => Width::W,
        0b011 => Width::D,
        _ => bail!("reserved AMO width {funct3:#05b}: {w:#010x}"),
    };
    let ord = Ordering {
        acquire: (w >> 26) & 1 != 0,
        release: (w >> 25) & 1 != 0,
    };

    let inst = match (w >> 27) & 0x1f {
        0b00010 if rs2.is_zero() => Inst::LoadReserved {
            width,
            rd,
            rs1,
            ord,
        },
        0b00011 => Inst::StoreConditional {
            width,
            rd,
            rs1,
            rs2,
            ord,
        },
        funct5 => {
            let op = match funct5 {
                0b00001 => AmoOp::Swap,
                0b00000 => AmoOp::Add,
                0b00100 => AmoOp::Xor,
                0b01100 => AmoOp::And,
                0b01000 => AmoOp::Or,
                0b10000 => AmoOp::Min,
                0b10100 => AmoOp::Max,
                0b11000 => AmoOp::MinU,
                0b11100 => AmoOp::MaxU,
                _ => bail!("reserved AMO funct5 {funct5:#07b}: {w:#010x}"),
            };
            Inst::Amo {
                op,
                width,
                rd,
                rs1,
                rs2,
                ord,
            }
        }
    };

    Ok(inst)
}

// -- 16-bit encodings ------------------------------------------------------

fn compressed(h: u16) -> Result<Inst> {
    let h = h as u32;
    let funct3 = (h >> 13) & 0x7;
    // Registers in the 3-bit compressed encoding address x8..x15.
    let rd_c = Reg::compressed((h >> 2) as u8);
    let rs1_c = Reg::compressed((h >> 7) as u8);
    // Registers in the full 5-bit slots.
    let rd = Reg::new((h >> 7) as u8);
    let rs2 = Reg::new((h >> 2) as u8);
    // The immediate shared by C.ADDI, C.LI, C.ANDI and friends.
    let imm_ci = sext(((((h >> 12) & 1) << 5) | ((h >> 2) & 0x1f)) as u64, 6);
    let shamt = ((((h >> 12) & 1) << 5) | ((h >> 2) & 0x1f)) as i64;

    let inst = match (h & 0x3, funct3) {
        // -- quadrant 0
        (0b00, 0b000) => {
            let imm = (((h >> 11) & 0x3) << 4)
                | (((h >> 7) & 0xf) << 6)
                | (((h >> 6) & 1) << 2)
                | (((h >> 5) & 1) << 3);
            if imm == 0 {
                bail!("reserved c.addi4spn with a zero immediate: {h:#06x}");
            }
            Inst::AluImm {
                op: AluOp::Add,
                rd: rd_c,
                rs1: Reg::SP,
                imm: imm as i64,
            }
        }
        (0b00, 0b010) => {
            let imm =
                ((((h >> 10) & 0x7) << 3) | (((h >> 6) & 1) << 2) | (((h >> 5) & 1) << 6)) as i64;
            Inst::Load {
                op: LoadOp::W,
                rd: rd_c,
                rs1: rs1_c,
                imm,
            }
        }
        (0b00, 0b011) => {
            let imm = ((((h >> 10) & 0x7) << 3) | (((h >> 5) & 0x3) << 6)) as i64;
            Inst::Load {
                op: LoadOp::D,
                rd: rd_c,
                rs1: rs1_c,
                imm,
            }
        }
        (0b00, 0b110) => {
            let imm =
                ((((h >> 10) & 0x7) << 3) | (((h >> 6) & 1) << 2) | (((h >> 5) & 1) << 6)) as i64;
            Inst::Store {
                op: StoreOp::W,
                rs1: rs1_c,
                rs2: rd_c,
                imm,
            }
        }
        (0b00, 0b111) => {
            let imm = ((((h >> 10) & 0x7) << 3) | (((h >> 5) & 0x3) << 6)) as i64;
            Inst::Store {
                op: StoreOp::D,
                rs1: rs1_c,
                rs2: rd_c,
                imm,
            }
        }

        // -- quadrant 1
        (0b01, 0b000) => Inst::AluImm {
            op: AluOp::Add,
            rd,
            rs1: rd,
            imm: imm_ci,
        },
        (0b01, 0b001) => {
            if rd.is_zero() {
                bail!("c.addiw with rd=x0 is reserved: {h:#06x}");
            }
            Inst::AluImmW {
                op: AluOp::Add,
                rd,
                rs1: rd,
                imm: imm_ci,
            }
        }
        (0b01, 0b010) => Inst::AluImm {
            op: AluOp::Add,
            rd,
            rs1: Reg::ZERO,
            imm: imm_ci,
        },
        (0b01, 0b011) if rd == Reg::SP => {
            let imm = sext(
                ((((h >> 12) & 1) << 9)
                    | (((h >> 6) & 1) << 4)
                    | (((h >> 5) & 1) << 6)
                    | (((h >> 3) & 0x3) << 7)
                    | (((h >> 2) & 1) << 5)) as u64,
                10,
            );
            if imm == 0 {
                bail!("c.addi16sp with a zero immediate is reserved: {h:#06x}");
            }
            Inst::AluImm {
                op: AluOp::Add,
                rd: Reg::SP,
                rs1: Reg::SP,
                imm,
            }
        }
        (0b01, 0b011) => {
            let imm = sext(
                ((((h >> 12) & 1) << 17) | (((h >> 2) & 0x1f) << 12)) as u64,
                18,
            );
            if imm == 0 {
                bail!("c.lui with a zero immediate is reserved: {h:#06x}");
            }
            Inst::Lui { rd, imm }
        }
        (0b01, 0b100) => match (h >> 10) & 0x3 {
            0b00 => Inst::AluImm {
                op: AluOp::Srl,
                rd: rs1_c,
                rs1: rs1_c,
                imm: shamt,
            },
            0b01 => Inst::AluImm {
                op: AluOp::Sra,
                rd: rs1_c,
                rs1: rs1_c,
                imm: shamt,
            },
            0b10 => Inst::AluImm {
                op: AluOp::And,
                rd: rs1_c,
                rs1: rs1_c,
                imm: imm_ci,
            },
            _ => {
                let op = match ((h >> 12) & 1, (h >> 5) & 0x3) {
                    (0, 0b00) => {
                        return Ok(Inst::Alu {
                            op: AluOp::Sub,
                            rd: rs1_c,
                            rs1: rs1_c,
                            rs2: rd_c,
                        });
                    }
                    (0, 0b01) => AluOp::Xor,
                    (0, 0b10) => AluOp::Or,
                    (0, 0b11) => AluOp::And,
                    (1, 0b00) => {
                        return Ok(Inst::AluW {
                            op: AluOp::Sub,
                            rd: rs1_c,
                            rs1: rs1_c,
                            rs2: rd_c,
                        });
                    }
                    (1, 0b01) => {
                        return Ok(Inst::AluW {
                            op: AluOp::Add,
                            rd: rs1_c,
                            rs1: rs1_c,
                            rs2: rd_c,
                        });
                    }
                    _ => bail!("reserved compressed ALU encoding: {h:#06x}"),
                };
                Inst::Alu {
                    op,
                    rd: rs1_c,
                    rs1: rs1_c,
                    rs2: rd_c,
                }
            }
        },
        (0b01, 0b101) => Inst::Jal {
            rd: Reg::ZERO,
            imm: cj_offset(h),
        },
        (0b01, 0b110) => Inst::Branch {
            op: Cond::Eq,
            rs1: rs1_c,
            rs2: Reg::ZERO,
            imm: cb_offset(h),
        },
        (0b01, 0b111) => Inst::Branch {
            op: Cond::Ne,
            rs1: rs1_c,
            rs2: Reg::ZERO,
            imm: cb_offset(h),
        },

        // -- quadrant 2
        (0b10, 0b000) => Inst::AluImm {
            op: AluOp::Sll,
            rd,
            rs1: rd,
            imm: shamt,
        },
        (0b10, 0b010) => {
            if rd.is_zero() {
                bail!("c.lwsp with rd=x0 is reserved: {h:#06x}");
            }
            let imm =
                ((((h >> 12) & 1) << 5) | (((h >> 4) & 0x7) << 2) | (((h >> 2) & 0x3) << 6)) as i64;
            Inst::Load {
                op: LoadOp::W,
                rd,
                rs1: Reg::SP,
                imm,
            }
        }
        (0b10, 0b011) => {
            if rd.is_zero() {
                bail!("c.ldsp with rd=x0 is reserved: {h:#06x}");
            }
            let imm =
                ((((h >> 12) & 1) << 5) | (((h >> 5) & 0x3) << 3) | (((h >> 2) & 0x7) << 6)) as i64;
            Inst::Load {
                op: LoadOp::D,
                rd,
                rs1: Reg::SP,
                imm,
            }
        }
        (0b10, 0b100) => match ((h >> 12) & 1, rd.is_zero(), rs2.is_zero()) {
            // c.jr
            (0, false, true) => Inst::Jalr {
                rd: Reg::ZERO,
                rs1: rd,
                imm: 0,
            },
            // c.mv
            (0, _, false) => Inst::Alu {
                op: AluOp::Add,
                rd,
                rs1: Reg::ZERO,
                rs2,
            },
            // c.ebreak
            (1, true, true) => Inst::Ebreak,
            // c.jalr
            (1, false, true) => Inst::Jalr {
                rd: Reg::RA,
                rs1: rd,
                imm: 0,
            },
            // c.add
            (1, _, false) => Inst::Alu {
                op: AluOp::Add,
                rd,
                rs1: rd,
                rs2,
            },
            _ => bail!("reserved compressed encoding: {h:#06x}"),
        },
        (0b10, 0b110) => {
            let imm = ((((h >> 9) & 0xf) << 2) | (((h >> 7) & 0x3) << 6)) as i64;
            Inst::Store {
                op: StoreOp::W,
                rs1: Reg::SP,
                rs2,
                imm,
            }
        }
        (0b10, 0b111) => {
            let imm = ((((h >> 10) & 0x7) << 3) | (((h >> 7) & 0x7) << 6)) as i64;
            Inst::Store {
                op: StoreOp::D,
                rs1: Reg::SP,
                rs2,
                imm,
            }
        }

        _ => bail!("unsupported compressed instruction: {h:#06x}"),
    };

    Ok(inst)
}

/// The CJ-format offset used by `c.j`.
fn cj_offset(h: u32) -> i64 {
    sext(
        ((((h >> 12) & 1) << 11)
            | (((h >> 11) & 1) << 4)
            | (((h >> 9) & 0x3) << 8)
            | (((h >> 8) & 1) << 10)
            | (((h >> 7) & 1) << 6)
            | (((h >> 6) & 1) << 7)
            | (((h >> 3) & 0x7) << 1)
            | (((h >> 2) & 1) << 5)) as u64,
        12,
    )
}

/// The CB-format offset used by `c.beqz` and `c.bnez`.
fn cb_offset(h: u32) -> i64 {
    sext(
        ((((h >> 12) & 1) << 8)
            | (((h >> 10) & 0x3) << 3)
            | (((h >> 5) & 0x3) << 6)
            | (((h >> 3) & 0x3) << 1)
            | (((h >> 2) & 1) << 5)) as u64,
        9,
    )
}
