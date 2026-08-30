// The link that turns the library into an image: cargo emits one for a bin
// target and an archive for a lib, and `extern crate` is what pulls the tools
// into it.
#![cfg_attr(any(target_arch = "wasm32", target_arch = "riscv64"), no_std, no_main)]

extern crate __CRATE__ as _;

#[cfg(not(any(target_arch = "wasm32", target_arch = "riscv64")))]
fn main() {}
