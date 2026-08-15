//! RISC-V general-purpose registers

use core::fmt;

/// The number of general-purpose registers.
pub const REGISTER_COUNT: usize = 32;

/// How many arguments the ABI passes in registers, `a0`..`a7`.
pub const REGISTER_ARGS: usize = 8;

/// A RISC-V general-purpose register, `x0`..`x31`.
///
/// `x0` is hardwired to zero; the translator relies on [`Reg::is_zero`] to
/// fold reads to a constant and drop writes.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Reg(u8);

macro_rules! regs {
    ($($name:ident = $index:expr, $abi:literal;)*) => {
        impl Reg {
            $(pub const $name: Reg = Reg($index);)*

            /// The ABI name, as printed by `objdump`.
            pub const fn abi(self) -> &'static str {
                match self.0 {
                    $($index => $abi,)*
                    _ => unreachable!(),
                }
            }
        }
    };
}

regs! {
    ZERO = 0, "zero"; RA = 1, "ra"; SP = 2, "sp"; GP = 3, "gp";
    TP = 4, "tp"; T0 = 5, "t0"; T1 = 6, "t1"; T2 = 7, "t2";
    S0 = 8, "s0"; S1 = 9, "s1"; A0 = 10, "a0"; A1 = 11, "a1";
    A2 = 12, "a2"; A3 = 13, "a3"; A4 = 14, "a4"; A5 = 15, "a5";
    A6 = 16, "a6"; A7 = 17, "a7"; S2 = 18, "s2"; S3 = 19, "s3";
    S4 = 20, "s4"; S5 = 21, "s5"; S6 = 22, "s6"; S7 = 23, "s7";
    S8 = 24, "s8"; S9 = 25, "s9"; S10 = 26, "s10"; S11 = 27, "s11";
    T3 = 28, "t3"; T4 = 29, "t4"; T5 = 30, "t5"; T6 = 31, "t6";
}

impl Reg {
    /// Build a register from a raw 5-bit encoding.
    pub const fn new(index: u8) -> Self {
        Reg(index & 0x1f)
    }

    /// Build a register from a compressed 3-bit encoding, which addresses
    /// `x8`..`x15`.
    pub const fn compressed(index: u8) -> Self {
        Reg((index & 0x7) + 8)
    }

    /// The register number, usable as an index into a register file.
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Whether this is `x0`, the constant-zero register.
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Debug for Reg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.abi())
    }
}

impl fmt::Display for Reg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.abi())
    }
}
