//! The host half of the system harnesses a host running more than one harness
//! serves.
//!
//! A harness belongs here only if it needs no policy invented to compile:
//! [`call`] takes its resolution as an argument and has none of its own. A
//! filesystem cannot be written without choosing a root, so it is written by
//! the host that chose one.

pub mod call;
pub mod store;
