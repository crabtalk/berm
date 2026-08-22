//! Publishing: prove who is asking, then prove what they are pointing at.

use crate::{Entry, Index};
use anyhow::{Context, Result, bail};
use berm_oci::{Access, Reference, Registry};
use serde_json::Value;
use std::str::FromStr;

const API: &str = "https://api.github.com";
/// GitHub refuses a request that does not identify itself.
const AGENT: &str = concat!("berm-registry/", env!("CARGO_PKG_VERSION"));

impl Index {
    /// Record one reference.
    ///
    /// Identity is checked before the artifact, so a stranger without a token
    /// cannot make this process go and fetch things on their behalf.
    pub async fn publish(&self, reference: &str, token: &str) -> Result<Entry> {
        let publisher = login(token).await?;
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

/// Who is asking, according to GitHub.
///
/// Identity is borrowed and checked per request: there is no account here to
/// create, lose or reset, and still a login to attribute a bad entry to.
async fn login(token: &str) -> Result<String> {
    let response = reqwest::Client::new()
        .get(format!("{API}/user"))
        .header("User-Agent", AGENT)
        .bearer_auth(token)
        .send()
        .await
        .context("cannot reach GitHub to check the token")?;
    if !response.status().is_success() {
        bail!("GitHub does not recognise that token");
    }

    let user: Value = response
        .json()
        .await
        .context("GitHub returned a user that is not JSON")?;
    user.get("login")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .context("GitHub returned a user without a login")
}
