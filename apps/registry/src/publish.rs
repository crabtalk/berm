//! Publishing: what is being pointed at has to be real.

use crate::{Entry, Index};
use anyhow::{Context, Result};
use berm_oci::{Access, Reference, Registry};
use std::str::FromStr;

impl Index {
    /// Record one reference.
    ///
    /// The artifact is what gets checked, not the publisher: an entry is a full
    /// OCI reference, and whoever holds that namespace already proved it to the
    /// registry that issued their push token. `publisher` is whatever the
    /// mounting service vouched for, and nothing here needs it to be anyone.
    pub async fn publish(&self, reference: &str, publisher: Option<String>) -> Result<Entry> {
        let reference = Reference::from_str(reference)?;
        let key = format!("{}/{}", reference.registry, reference.repository);
        let name = reference.to_string();

        // Anonymously, always: the index has no business holding credentials
        // for someone else's registry, and a harness nobody can pull is not one
        // to list.
        let (digest, manifest) = tokio::task::spawn_blocking(move || {
            let registry = Registry::open(&reference, Access::Read)?;
            registry.describe(&reference.reference)
        })
        .await
        .context("the registry lookup panicked")??;

        let entry = Entry::new(name, digest, publisher, manifest);
        self.store.append(&key, &entry).await?;

        self.entries
            .write()
            .await
            .entry(key)
            .or_default()
            .push(entry.clone());
        Ok(entry)
    }
}
