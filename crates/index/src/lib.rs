//! A list of published programs, and how to search one.
//!
//! A registry holds the bytes; an index holds the list, because no registry API
//! will tell you who published a program. The list is a git repository — one
//! JSON Lines file per program, one line per version — so a copy of it is a
//! clone, and reading one needs no service and no credential.
//!
//! [`Index`] itself reaches nothing: a caller fetches an artifact, builds an
//! [`Entry`] from what it said it was, and this holds and searches the result.
//! [`Source`] is what goes and gets one, from a clone or from a service.

mod entry;
mod index;
mod source;

pub use entry::Entry;
pub use index::Index;
pub use source::{DEFAULT, Source};
