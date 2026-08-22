//! Where the index is kept.
//!
//! One JSON Lines file per harness repository, named for it —
//! `ghcr.io/clearloop/fs.json`. Appending a version is one line, so a diff
//! stays readable and two harnesses never touch the same file, which is what
//! keeps concurrent publishes from colliding.

use crate::Entry;
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const API: &str = "https://api.github.com";
/// GitHub refuses a request that does not identify itself.
const AGENT: &str = concat!("berm-registry/", env!("CARGO_PKG_VERSION"));

pub struct Store {
    http: reqwest::Client,
    kind: Kind,
}

/// A directory is what a local index and a first look use; a repository is what
/// a deployment uses. The tree is identical either way, so moving between them
/// is a copy.
enum Kind {
    Directory(PathBuf),
    Github { repository: String, token: String },
}

impl Store {
    /// A directory if one is there, a GitHub repository otherwise — the same
    /// rule `berm deploy` uses to tell a file from a registry reference.
    pub fn open(index: &str, token: Option<String>) -> Result<Self> {
        let kind = match Path::new(index).is_dir() {
            true => Kind::Directory(PathBuf::from(index)),
            false => Kind::Github {
                repository: index.to_owned(),
                token: token.context(
                    "publishing to a GitHub index needs a token; set GITHUB_TOKEN, or point \
                     --index at a directory",
                )?,
            },
        };
        Ok(Self {
            http: reqwest::Client::new(),
            kind,
        })
    }

    /// The whole index, by harness repository. Read once at boot and again
    /// whenever the process restarts, which is the only reload there is.
    pub async fn load(&self) -> Result<BTreeMap<String, Vec<Entry>>> {
        let mut index = BTreeMap::new();
        for (key, body) in self.files().await? {
            let entries: Vec<Entry> = body
                .lines()
                .filter(|line| !line.trim().is_empty())
                .filter_map(|line| match serde_json::from_str(line) {
                    Ok(entry) => Some(entry),
                    // One unreadable line is not worth refusing to start over:
                    // the rest of the index is still true.
                    Err(error) => {
                        tracing::warn!(key, %error, "skipping an unreadable entry");
                        None
                    }
                })
                .collect();
            if !entries.is_empty() {
                index.insert(key, entries);
            }
        }
        Ok(index)
    }

    /// Add one version. The file is read, extended and written whole, because
    /// neither a filesystem nor the Contents API offers an append.
    pub async fn append(&self, key: &str, entry: &Entry) -> Result<()> {
        let line = serde_json::to_string(entry)?;
        match &self.kind {
            Kind::Directory(root) => {
                let path = root.join(format!("{key}.json"));
                let parent = path.parent().context("index path has no parent")?;
                tokio::fs::create_dir_all(parent).await?;
                let mut body = tokio::fs::read_to_string(&path).await.unwrap_or_default();
                body.push_str(&line);
                body.push('\n');
                tokio::fs::write(&path, body).await?;
                Ok(())
            }
            Kind::Github { repository, token } => {
                let path = format!("{key}.json");
                let held = self.contents(repository, token, &path).await?;
                let body = match &held {
                    Some((body, _)) => format!("{body}{line}\n"),
                    None => format!("{line}\n"),
                };

                let mut request = json!({
                    "message": format!("publish {}", entry.reference),
                    "content": STANDARD.encode(body),
                });
                // Absent for a new file; on an existing one it is what makes a
                // concurrent write fail loudly instead of overwriting.
                if let Some((_, sha)) = held {
                    request["sha"] = sha.into();
                }

                let response = self
                    .http
                    .put(format!("{API}/repos/{repository}/contents/{path}"))
                    .header("User-Agent", AGENT)
                    .bearer_auth(token)
                    .json(&request)
                    .send()
                    .await
                    .context("cannot reach GitHub")?;
                if !response.status().is_success() {
                    let status = response.status();
                    bail!("GitHub refused the index write: {status}");
                }
                Ok(())
            }
        }
    }

    /// Every index file, keyed by the harness repository it holds.
    async fn files(&self) -> Result<Vec<(String, String)>> {
        match &self.kind {
            Kind::Directory(root) => {
                let mut files = Vec::new();
                walk(root, root, &mut files)?;
                Ok(files)
            }
            Kind::Github { repository, token } => {
                let tree: Value = self
                    .http
                    .get(format!(
                        "{API}/repos/{repository}/git/trees/HEAD?recursive=1"
                    ))
                    .header("User-Agent", AGENT)
                    .bearer_auth(token)
                    .send()
                    .await
                    .context("cannot reach GitHub")?
                    .json()
                    .await
                    .context("GitHub returned a tree that is not JSON")?;

                let mut files = Vec::new();
                let paths = tree.get("tree").and_then(Value::as_array);
                for node in paths.into_iter().flatten() {
                    let Some(path) = node.get("path").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some(key) = path.strip_suffix(".json") else {
                        continue;
                    };
                    if let Some((body, _)) = self.contents(repository, token, path).await? {
                        files.push((key.to_owned(), body));
                    }
                }
                Ok(files)
            }
        }
    }

    /// A file's text and the blob sha a write has to quote back.
    async fn contents(
        &self,
        repository: &str,
        token: &str,
        path: &str,
    ) -> Result<Option<(String, String)>> {
        let response = self
            .http
            .get(format!("{API}/repos/{repository}/contents/{path}"))
            .header("User-Agent", AGENT)
            .bearer_auth(token)
            .send()
            .await
            .context("cannot reach GitHub")?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let file: Value = response
            .json()
            .await
            .context("GitHub returned a file that is not JSON")?;
        let encoded = file
            .get("content")
            .and_then(Value::as_str)
            .context("GitHub returned a file without content")?;
        let sha = file
            .get("sha")
            .and_then(Value::as_str)
            .context("GitHub returned a file without a sha")?;

        // GitHub wraps base64 at 60 columns, which the decoder will not take.
        let raw = STANDARD
            .decode(encoded.replace('\n', ""))
            .context("GitHub returned content that is not base64")?;
        let body = String::from_utf8(raw).context("an index file is not UTF-8")?;
        Ok(Some((body, sha.to_owned())))
    }
}

/// Every `.json` under `root`, keyed by its path with the suffix dropped.
fn walk(root: &Path, at: &Path, found: &mut Vec<(String, String)>) -> Result<()> {
    for entry in std::fs::read_dir(at).with_context(|| format!("cannot read {}", at.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            walk(root, &path, found)?;
            continue;
        }
        let Some(key) = path
            .strip_prefix(root)
            .ok()
            .and_then(Path::to_str)
            .and_then(|path| path.strip_suffix(".json"))
        else {
            continue;
        };
        found.push((key.to_owned(), std::fs::read_to_string(&path)?));
    }
    Ok(())
}
