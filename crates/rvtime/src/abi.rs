//! Moving values between Rust and the guest's argument registers
//!
//! The RISC-V calling convention passes arguments and results in `a0`..`a7`,
//! all of them plain 64-bit words, so there is no type information to encode
//! the way a WebAssembly signature would. A tuple's arity is what picks the
//! registers.

use rv::Reg;

/// A value or tuple carried in the argument registers.
pub trait Regs: Sized {
    /// How many registers the value occupies.
    const COUNT: usize;

    /// Read the value out of a register file.
    fn read(regs: &[u64; 32]) -> Self;

    /// Write the value into a register file.
    fn write(self, regs: &mut [u64; 32]);
}

impl Regs for () {
    const COUNT: usize = 0;

    fn read(_: &[u64; 32]) -> Self {}
    fn write(self, _: &mut [u64; 32]) {}
}

impl Regs for u64 {
    const COUNT: usize = 1;

    fn read(regs: &[u64; 32]) -> Self {
        regs[Reg::A0.index()]
    }

    fn write(self, regs: &mut [u64; 32]) {
        regs[Reg::A0.index()] = self;
    }
}

macro_rules! tuple_regs {
    ($count:expr; $($name:ident : $ty:ty = $index:expr),+) => {
        impl Regs for ($($ty,)+) {
            const COUNT: usize = $count;

            fn read(regs: &[u64; 32]) -> Self {
                ($(regs[Reg::A0.index() + $index],)+)
            }

            fn write(self, regs: &mut [u64; 32]) {
                let ($($name,)+) = self;
                $(regs[Reg::A0.index() + $index] = $name;)+
            }
        }
    };
}

tuple_regs!(1; a: u64 = 0);
tuple_regs!(2; a: u64 = 0, b: u64 = 1);
tuple_regs!(3; a: u64 = 0, b: u64 = 1, c: u64 = 2);
tuple_regs!(4; a: u64 = 0, b: u64 = 1, c: u64 = 2, d: u64 = 3);
tuple_regs!(5; a: u64 = 0, b: u64 = 1, c: u64 = 2, d: u64 = 3, e: u64 = 4);
tuple_regs!(6; a: u64 = 0, b: u64 = 1, c: u64 = 2, d: u64 = 3, e: u64 = 4, f: u64 = 5);
tuple_regs!(7; a: u64 = 0, b: u64 = 1, c: u64 = 2, d: u64 = 3, e: u64 = 4, f: u64 = 5, g: u64 = 6);
tuple_regs!(8; a: u64 = 0, b: u64 = 1, c: u64 = 2, d: u64 = 3, e: u64 = 4, f: u64 = 5, g: u64 = 6, h: u64 = 7);
