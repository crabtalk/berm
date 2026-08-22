//! Moving values between Rust and the guest's argument registers.

use rvtime::{Reg, Regs};

#[test]
fn tuples_map_onto_consecutive_argument_registers() {
    let mut regs = [0u64; 32];
    (10u64, 20u64, 30u64).write(&mut regs);

    assert_eq!(regs[Reg::A0.index()], 10);
    assert_eq!(regs[Reg::A1.index()], 20);
    assert_eq!(regs[Reg::A2.index()], 30);
    assert_eq!(<(u64, u64, u64)>::read(&regs), (10, 20, 30));
}

#[test]
fn the_unit_type_uses_no_registers() {
    let mut regs = [7u64; 32];
    ().write(&mut regs);
    assert_eq!(regs[Reg::A0.index()], 7, "unit must not clobber a0");
    assert_eq!(<()>::COUNT, 0);
}

#[test]
fn a_bare_u64_uses_a0() {
    let mut regs = [0u64; 32];
    42u64.write(&mut regs);
    assert_eq!(regs[Reg::A0.index()], 42);
    assert_eq!(u64::read(&regs), 42);
}

#[test]
fn the_widest_tuple_fills_a0_through_a7() {
    let mut regs = [0u64; 32];
    (1u64, 2, 3, 4, 5, 6, 7, 8).write(&mut regs);

    for (offset, expected) in (1u64..=8).enumerate() {
        assert_eq!(regs[Reg::A0.index() + offset], expected);
    }
    assert_eq!(<(u64, u64, u64, u64, u64, u64, u64, u64)>::COUNT, 8);
}
