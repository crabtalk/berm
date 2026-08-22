//! `berm` — the command-line client for bermd.

pub use client::Client;

mod client;
pub mod cmd;
mod http;
pub mod index;
