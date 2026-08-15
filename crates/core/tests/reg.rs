//! Register naming and encoding.

use rvtime_core::Reg;

#[test]
fn abi_names() {
    assert_eq!(Reg::new(0), Reg::ZERO);
    assert_eq!(Reg::new(2).abi(), "sp");
    assert_eq!(Reg::new(10).abi(), "a0");
    assert_eq!(Reg::new(31).abi(), "t6");
}

#[test]
fn compressed_addresses_x8_through_x15() {
    assert_eq!(Reg::compressed(0), Reg::S0);
    assert_eq!(Reg::compressed(7), Reg::A5);
}

#[test]
fn only_x0_is_zero() {
    assert!(Reg::ZERO.is_zero());
    for n in 1..32u8 {
        assert!(!Reg::new(n).is_zero());
    }
}

#[test]
fn debug_output_matches_objdump() {
    // Printing ABI names is what makes the decoder's differential test
    // readable against `llvm-objdump`.
    assert_eq!(format!("{:?}", Reg::SP), "sp");
    assert_eq!(format!("{}", Reg::A0), "a0");
}
