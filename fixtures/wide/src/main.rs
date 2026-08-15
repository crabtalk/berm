//! Decoder coverage fixture.
//!
//! Compiled Rust only emits the instructions LLVM happens to choose, which
//! leaves most of RV64IMAC untested. This fixture spells the encodings out
//! directly so the differential test covers the whole ISA.
//!
//! `coverage` is never executed — it exists to be disassembled. Its body is
//! deliberately nonsense as a program; only the encodings matter.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::ptr::write_volatile;

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}

core::arch::global_asm!(
    r#"
    .section .text.coverage,"ax",@progbits
    # Inline asm starts at the base ISA regardless of the target triple.
    .option arch, +m, +a, +c
    .globl coverage
    .type coverage,@function
coverage:

    # -- 32-bit encodings. norvc stops the assembler compressing them.
    .option push
    .option norvc

    lb    a0, 0(a1)
    lh    a0, 2(a1)
    lw    a0, 4(a1)
    ld    a0, 8(a1)
    lbu   a0, 1(a1)
    lhu   a0, 6(a1)
    lwu   a0, 12(a1)

    sb    a2, 0(a1)
    sh    a2, 2(a1)
    sw    a2, 4(a1)
    sd    a2, 8(a1)

1:  beq   a0, a1, 1b
    bne   a0, a1, 1b
    blt   a0, a1, 1b
    bge   a0, a1, 1b
    bltu  a0, a1, 1b
    bgeu  a0, a1, 1b

    addi  a0, a1, -2048
    slti  a0, a1, 17
    sltiu a0, a1, 17
    xori  a0, a1, 255
    ori   a0, a1, 255
    andi  a0, a1, -256
    slli  a0, a1, 63
    srli  a0, a1, 63
    srai  a0, a1, 63

    add   a0, a1, a2
    sub   a0, a1, a2
    sll   a0, a1, a2
    slt   a0, a1, a2
    sltu  a0, a1, a2
    xor   a0, a1, a2
    srl   a0, a1, a2
    sra   a0, a1, a2
    or    a0, a1, a2
    and   a0, a1, a2

    addiw a0, a1, -1
    slliw a0, a1, 31
    srliw a0, a1, 31
    sraiw a0, a1, 31
    addw  a0, a1, a2
    subw  a0, a1, a2
    sllw  a0, a1, a2
    srlw  a0, a1, a2
    sraw  a0, a1, a2

    mul    a0, a1, a2
    mulh   a0, a1, a2
    mulhsu a0, a1, a2
    mulhu  a0, a1, a2
    div    a0, a1, a2
    divu   a0, a1, a2
    rem    a0, a1, a2
    remu   a0, a1, a2

    mulw  a0, a1, a2
    divw  a0, a1, a2
    divuw a0, a1, a2
    remw  a0, a1, a2
    remuw a0, a1, a2

    lui   a0, 0xfffff
    auipc a0, 0x1
    jal   ra, 2f
2:  jalr  ra, 4(a1)

    lr.w        a0, (a1)
    lr.d        a0, (a1)
    sc.w        a0, a2, (a1)
    sc.d        a0, a2, (a1)
    lr.w.aq     a0, (a1)
    sc.d.aqrl   a0, a2, (a1)

    amoswap.w  a0, a2, (a1)
    amoswap.d  a0, a2, (a1)
    amoadd.w   a0, a2, (a1)
    amoadd.d   a0, a2, (a1)
    amoxor.w   a0, a2, (a1)
    amoxor.d   a0, a2, (a1)
    amoand.w   a0, a2, (a1)
    amoand.d   a0, a2, (a1)
    amoor.w    a0, a2, (a1)
    amoor.d    a0, a2, (a1)
    amomin.w   a0, a2, (a1)
    amomin.d   a0, a2, (a1)
    amomax.w   a0, a2, (a1)
    amomax.d   a0, a2, (a1)
    amominu.w  a0, a2, (a1)
    amominu.d  a0, a2, (a1)
    amomaxu.w  a0, a2, (a1)
    amomaxu.d  a0, a2, (a1)
    amoadd.w.aq   a0, a2, (a1)
    amoadd.d.rl   a0, a2, (a1)

    ecall
    ebreak
    fence
    fence.i

    .option pop

    # -- compressed encodings, spelled explicitly
    c.addi4spn a0, sp, 8
    c.lw       a0, 4(a1)
    c.ld       a0, 8(a1)
    c.sw       a0, 4(a1)
    c.sd       a0, 8(a1)

    c.nop
    c.addi     a0, 1
    c.addiw    a0, 1
    c.li       a0, -1
    c.addi16sp sp, 32
    c.lui      a0, 1
    c.srli     a0, 1
    c.srai     a0, 1
    c.andi     a0, -1
    c.sub      a0, a1
    c.xor      a0, a1
    c.or       a0, a1
    c.and      a0, a1
    c.subw     a0, a1
    c.addw     a0, a1
    c.j        3f
3:  c.beqz     a0, 3b
    c.bnez     a0, 3b

    c.slli     a0, 1
    c.lwsp     a0, 4(sp)
    c.ldsp     a0, 8(sp)
    c.mv       a0, a1
    c.add      a0, a1
    c.swsp     a0, 4(sp)
    c.sdsp     a0, 8(sp)
    c.jalr     a1
    c.ebreak
    c.jr       ra
    .size coverage, .-coverage
"#
);

unsafe extern "C" {
    fn coverage();
}

/// Keeps `coverage` alive against `--gc-sections`.
#[no_mangle]
pub static COVERAGE: unsafe extern "C" fn() = coverage;

#[no_mangle]
pub static mut SINK: u64 = 0;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe { write_volatile(&raw mut SINK, COVERAGE as usize as u64) };
    loop {}
}
