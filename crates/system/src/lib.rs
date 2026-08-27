//! The host half of the system harnesses a host running more than one harness
//! serves.
//!
//! A harness belongs here only if it needs no policy invented to compile:
//! [`store`] takes its persistence as an argument and has none of its own. A
//! filesystem cannot be written without choosing a root, so it is written by
//! the host that chose one.
//!
//! `berm.call` is not here: resolving a name needs only the set berm already
//! holds, so berm serves it.

pub mod store;
