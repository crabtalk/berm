//! What a guest can reach.
//!
//! A system harness is native host code behind a name, the way a precompile is
//! native code behind an address. berm serves the ones whose whole behaviour is
//! the harness model — [`call`], which resolves against the set berm already
//! holds, and [`store`], whose keyspace is whoever is asking. Where the bytes
//! land is still a host's, and arrives as an argument.
//!
//! Everything else is a [`crate::System`] the embedder registers. berm ships no
//! filesystem, no command runner and no network: each needs a policy invented
//! to compile — a root, a size cap, an allowlist — and those are decisions
//! about a host.

pub mod call;
pub mod store;
