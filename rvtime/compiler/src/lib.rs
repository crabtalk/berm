//! Codegen backends, guest memory, and trap handling.

#[cfg(not(any(feature = "jit", feature = "aot")))]
compile_error!("rvtime-compiler needs at least one of the `jit` and `aot` features");

pub use crate::{
    cache::Cache,
    engine::{Engine, OptLevel},
    memory::Memory,
    module::Module,
    trap::Fault,
};

#[cfg(feature = "aot")]
mod aot;
#[cfg(feature = "aot")]
mod artifact;
pub mod cache;
pub mod engine;
#[cfg(feature = "jit")]
mod jit;
#[cfg(feature = "aot")]
mod mapping;
pub mod memory;
pub mod module;
pub mod trap;
