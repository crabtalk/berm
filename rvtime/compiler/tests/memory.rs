//! Mapping guest memory and the guard pages around it.

use rv::{Perms, Program, Segment};
use rvtime_compiler::{Memory, memory::host_page, trap};
use std::ptr;

/// Address space used by these tests. Small on purpose -- the point of the
/// size being configurable is that a test guest need not reserve gigabytes.
fn memory_size() -> u64 {
    16 << 20
}

fn stack_size() -> u64 {
    host_page() * 64
}

fn program() -> Program {
    rv::elf::load(include_bytes!("../../../fixtures/basic.elf")).expect("loads")
}

/// A program with each segment on its own host page, so permissions are not
/// merged and the protection mechanism can be tested directly.
fn isolated() -> Program {
    let page = host_page();
    Program {
        entry: page,
        text: page..page * 2,
        functions: Default::default(),
        indirect: Default::default(),
        symbols: Default::default(),
        segments: vec![
            Segment {
                addr: page,
                data: vec![0x11; 16],
                size: page,
                perms: Perms {
                    read: true,
                    write: false,
                    exec: true,
                },
            },
            Segment {
                addr: page * 3,
                data: vec![0x22; 16],
                size: page,
                perms: Perms {
                    read: true,
                    write: true,
                    exec: false,
                },
            },
        ],
    }
}

#[test]
fn maps_the_program_image() {
    let program = program();
    let memory = Memory::new(&program, memory_size(), stack_size()).expect("maps");

    // The first instruction of op_add must be readable at its guest address.
    let op_add = program.symbols["op_add"];
    let bytes = memory.read(op_add, 4).expect("readable");
    let (inst, _) = rv::decode(bytes).expect("decodes");
    assert_eq!(
        inst,
        rv::Inst::Alu {
            op: rv::AluOp::Add,
            rd: rv::Reg::A0,
            rs1: rv::Reg::A0,
            rs2: rv::Reg::A1,
        }
    );
}

#[test]
fn stack_is_writable_and_aligned() {
    let program = program();
    let mut memory = Memory::new(&program, memory_size(), stack_size()).expect("maps");

    let sp = memory.stack_pointer();
    assert_eq!(sp % 16, 0, "stack pointer must be 16-byte aligned");

    memory.write(sp - 8, &[0xab; 8]).expect("writable");
    assert_eq!(memory.read(sp - 8, 8).expect("readable"), &[0xab; 8]);
}

#[test]
fn unmapped_addresses_fault() {
    let program = program();
    let memory = Memory::new(&program, memory_size(), stack_size()).expect("maps");
    trap::set_guest_region(memory.base() as usize, memory.size());

    // The only uncommitted gap is the guard page between heap and stack.
    let hole = memory.heap().end;
    let base = memory.base();
    let fault = trap::protect(|| unsafe { ptr::read_volatile(base.add(hole as usize)) })
        .expect_err("guard page must fault");

    assert_eq!(fault.guest, Some(hole));
}

#[test]
fn running_off_the_stack_faults() {
    let program = program();
    let memory = Memory::new(&program, memory_size(), stack_size()).expect("maps");
    trap::set_guest_region(memory.base() as usize, memory.size());

    // Everything below the stack region is uncommitted.
    let below = memory_size() - stack_size() - 8;
    let base = memory.base();
    let fault = trap::protect(|| unsafe { ptr::read_volatile(base.add(below as usize)) })
        .expect_err("running off the stack must fault");

    assert_eq!(fault.guest, Some(below));
}

#[test]
fn executable_segments_are_mapped_read_only() {
    // The exec bit is dropped when mapping, so an executable segment on its own
    // page comes out readable and not writable. Compiled code lives in the
    // JIT's pages; guest memory never needs to be executable.
    let program = isolated();
    let memory = Memory::new(&program, memory_size(), stack_size()).expect("maps");
    trap::set_guest_region(memory.base() as usize, memory.size());

    let code = program.text.start;
    let base = memory.base();

    assert_eq!(memory.read(code, 1).expect("readable")[0], 0x11);
    let fault = trap::protect(|| unsafe { ptr::write_volatile(base.add(code as usize), 0) })
        .expect_err("writing to code must fault");
    assert_eq!(fault.guest, Some(code));
}

#[test]
fn writable_segments_accept_writes() {
    let program = isolated();
    let memory = Memory::new(&program, memory_size(), stack_size()).expect("maps");
    trap::set_guest_region(memory.base() as usize, memory.size());

    let data = host_page() * 3;
    let base = memory.base();
    trap::protect(|| unsafe { ptr::write_volatile(base.add(data as usize), 0x55) })
        .expect("data segment is writable");
    assert_eq!(memory.read(data, 1).expect("readable")[0], 0x55);
}

#[test]
fn rejects_a_misaligned_stack_size() {
    let program = program();
    assert!(Memory::new(&program, memory_size(), 0).is_err());
    assert!(Memory::new(&program, memory_size(), host_page() + 1).is_err());
}

#[test]
fn rejects_out_of_bounds_access() {
    let program = program();
    let memory = Memory::new(&program, memory_size(), stack_size()).expect("maps");
    assert!(memory.read(memory_size() - 4, 8).is_err());
    assert!(memory.read(u64::MAX, 1).is_err());
}
