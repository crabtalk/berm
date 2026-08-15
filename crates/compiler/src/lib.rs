//! Codegen backends, guest memory, and trap handling.

pub use crate::{
    cache::Cache,
    engine::{Engine, OptLevel},
    memory::Memory,
    module::Module,
    trap::Fault,
};

pub mod cache;
pub mod engine;
pub mod memory;
pub mod module;
pub mod trap;
