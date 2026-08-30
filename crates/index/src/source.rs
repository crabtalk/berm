//! Reaching an index.
//!
//! A clone of the list is a directory and a service that publishes into one is
//! a URL, so what a caller names takes either shape — the same way `berm
//! deploy` takes a file or a registry reference.

use crate::Entry;
use anyhow::{Context, Result, bail};
use berm_api::Failed;
use reqwest::blocking::{Client, Response};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Where to look, when nothing is named.
const ENV: &str = "BERM_INDEX";

/// The list read when nothing else is named.
pub const DEFAULT: &str = "https://github.com/crabtalk/berm-index.git";

pub enum Source {
    /// A clone of the list. Reading it needs no service and no credential.
    Local(PathBuf),
    Remote {
        host: String,
        http: Client,
    },
}

impl Source {
    /// Blocking, and a `.git` URL clones the first time — so this belongs off
    /// whatever thread paints, and off the path a keystroke takes.
    pub fn new(index: Option<&str>) -> Result<Self> {
        let index = match index {
            Some(index) => index.to_owned(),
            None => std::env::var(ENV).unwrap_or_else(|_| DEFAULT.to_owned()),
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
            http: Client::new(),
        })
    }

    pub fn search(&self, term: &str) -> Result<Vec<Entry>> {
        match self {
            Self::Local(root) => Ok(crate::Index::load(root)?
                .search(term)
                .into_iter()
                .cloned()
                .collect()),
            Self::Remote { host, http } => {
                let request = http.get(format!("{host}/berm")).query(&[("q", term)]);
                read(host, request.send())?
                    .json()
                    .context("the index returned something that is not a program list")
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
        read(host, request.send())?
            .json()
            .context("the index returned something that is not a program")
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

/// Turn a refusal into the message the index sent, rather than a status code
/// the caller then has to go and look up.
fn read(host: &str, response: reqwest::Result<Response>) -> Result<Response> {
    let response = response.with_context(|| format!("cannot reach {host}"))?;
    if response.status().is_success() {
        return Ok(response);
    }

    let status = response.status();
    match response.json::<Failed>() {
        Ok(failed) => bail!("{}", failed.error),
        Err(_) => bail!("{host} answered {status}"),
    }
}
