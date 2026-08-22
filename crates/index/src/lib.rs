//! A list of published harnesses, and how to search one.
//!
//! A registry holds the bytes; an index holds the list, because no registry API
//! will tell you who published a harness. The list is a git repository — one
//! JSON Lines file per harness, one line per version — so a copy of it is a
//! clone, and reading one needs no service and no credential.
//!
//! Nothing here reaches the network. A caller fetches an artifact, builds an
//! [`Entry`] from what it said it was, and this holds and searches the result.

mod entry;
mod index;

pub use entry::Entry;
pub use index::Index;
