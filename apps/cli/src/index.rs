//! Reaching an index.
//!
//! Which index is the caller's choice: there is no default here, because a
//! built-in one would make berm ship an opinion about whose list you read.
//!
//! A clone of the index is a directory, and a service that publishes into one
//! is a URL — so `--index` takes either, the same way `berm deploy` takes a
//! file or a registry reference.

use crate::http;
use anyhow::{Context, Result, bail};
use berm_index::Entry;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Where to look, when `--index` is not given.
const INDEX: &str = "BERM_INDEX";

/// The list read when nothing else is named.
const DEFAULT: &str = "https://github.com/crabtalk/berm-index.git";

pub enum Index {
    /// A clone of the list. Reading it needs no service and no credential.
    Local(PathBuf),
    Remote {
        host: String,
        http: reqwest::blocking::Client,
    },
}

impl Index {
    pub fn new(index: Option<&String>) -> Result<Self> {
        let index = match index {
            Some(index) => index.clone(),
            None => std::env::var(INDEX).unwrap_or_else(|_| DEFAULT.to_owned()),
        };

        if Path::new(&index).is_dir() {
            return Ok(Self::Local(PathBuf::from(index)));
        }
        // A `.git` URL is a list to copy, not a service to ask. Everything else
        // that is not already a directory is a service.
        if index.ends_with(".git") {
            return Ok(Self::Local(mirror(&index)?));
        }
        Ok(Self::Remote {
            host: index.trim_end_matches('/').to_owned(),
            http: reqwest::blocking::Client::new(),
        })
    }

    pub fn search(&self, term: &str) -> Result<Vec<Entry>> {
        match self {
            Self::Local(root) => Ok(berm_index::Index::load(root)?
                .search(term)
                .into_iter()
                .cloned()
                .collect()),
            Self::Remote { host, http } => {
                let request = http.get(format!("{host}/berm")).query(&[("q", term)]);
                http::read(host, request.send())?
                    .json()
                    .context("the index returned something that is not a harness list")
            }
        }
    }

    /// The token is whatever the index in front of you wants, and an open one
    /// wants none — so it rides along when there is one and is not invented
    /// when there is not.
    pub fn publish(&self, reference: &str, token: Option<&str>) -> Result<Entry> {
        let Self::Remote { host, http } = self else {
            bail!("publishing needs a service to publish to, not a clone of the list");
        };

        let mut request = http
            .post(format!("{host}/berm"))
            .json(&serde_json::json!({ "reference": reference }));
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        http::read(host, request.send())?
            .json()
            .context("the index returned something that is not a harness")
    }
}

/// The local copy of a list, cloned if it is not there yet.
///
/// Keyed by the URL so two indexes never share a directory. Cloned shallow —
/// what matters is the list, not how it came to be — and left alone afterwards,
/// so a search never waits on a network. `git -C <path> pull` refreshes it.
fn mirror(url: &str) -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    let path = url
        .split_once("://")
        .map_or(url, |(_, rest)| rest)
        .trim_end_matches(".git");
    let into = PathBuf::from(home).join(".berm/index").join(path);
    if into.is_dir() {
        return Ok(into);
    }

    let status = Command::new("git")
        .args(["clone", "--quiet", "--depth", "1", url])
        .arg(&into)
        .status()
        .context("cannot run git, which is what copies an index")?;
    if !status.success() {
        bail!("git could not clone {url}");
    }
    Ok(into)
}
