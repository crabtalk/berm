//! Target configuration.

use rvtime_compiler::{Engine, OptLevel};

#[test]
fn builds_for_both_optimisation_levels() {
    assert!(Engine::new(OptLevel::None).is_ok());
    assert!(Engine::new(OptLevel::Speed).is_ok());
}

#[test]
fn targets_a_64_bit_host() {
    let engine = Engine::default();
    assert_eq!(engine.isa().pointer_bits(), 64);
}
