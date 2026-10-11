//! `berm` — the command-line client for bermd and Cloud.

pub use client::Client;

mod client;
pub mod cmd;
pub mod context;
mod http;
