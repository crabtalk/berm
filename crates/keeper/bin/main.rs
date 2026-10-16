// The link that turns the library into an image: cargo emits one for a bin
// target and an archive for a lib, and `extern crate` is what pulls the tools
// into it.
#![cfg_attr(target_arch = "wasm32", no_std, no_main)]

extern crate berm_keeper as _;

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
