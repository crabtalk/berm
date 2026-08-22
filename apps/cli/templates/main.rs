// The link that turns the library into an ELF: cargo emits an image for a bin
// target and an archive for a lib, and `extern crate` is what pulls the tools
// in for `_start` to anchor.
#![cfg_attr(target_arch = "riscv64", no_std, no_main)]

extern crate __CRATE__ as _;

#[cfg(not(target_arch = "riscv64"))]
fn main() {}
