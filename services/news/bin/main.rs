// The image: cargo links an image for a bin target, and `extern crate` pulls
// the tools into it.
#![cfg_attr(target_arch = "wasm32", no_std, no_main)]

extern crate news as _;

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
