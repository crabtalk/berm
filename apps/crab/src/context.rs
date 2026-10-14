//! Where `crab` sends its calls: a named host, and for Cloud, the token and
//! the account it deploys under. Kept in `~/.crab/contexts.toml`.

use anyhow::{Context as _, Result, bail};
use crabtalk_sdk::client::{Client, Owner};
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{collections::BTreeMap, fs, path::PathBuf};

/// The context that is always there: a bermd on this machine.
pub const DEFAULT: &str = "default";

/// Where bermd listens by default.
const LOCAL: &str = "http://127.0.0.1:7777";

/// The file, under `~/.crab`.
const FILE: &str = "contexts.toml";

/// One target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// The account a Cloud context deploys under. A context without one is a
    /// bermd.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<Owner>,
}

/// Every context but [`DEFAULT`], and which one is in use.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Contexts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    #[serde(default)]
    pub contexts: BTreeMap<String, Context>,
}

impl Contexts {
    /// What the file holds, or nothing when there is none.
    pub fn load() -> Result<Self> {
        let path = path()?;
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(error).with_context(|| format!("cannot read {}", path.display()));
            }
        };
        toml::from_str(&text).with_context(|| format!("{} is not valid", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        let path = path()?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
        }
        fs::write(&path, toml::to_string(self)?)
            .with_context(|| format!("cannot write {}", path.display()))?;
        // It holds tokens.
        #[cfg(unix)]
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("cannot restrict {}", path.display()))?;
        Ok(())
    }

    /// The context `name` names, or the one in use when `name` is `None`.
    pub fn get(&self, name: Option<&str>) -> Result<Context> {
        let name = name.or(self.current.as_deref()).unwrap_or(DEFAULT);
        if name == DEFAULT {
            return Ok(Context {
                host: LOCAL.to_owned(),
                token: None,
                owner: None,
            });
        }
        match self.contexts.get(name) {
            Some(context) => Ok(context.clone()),
            None => bail!("no context named {name:?}; `crab context ls` lists them"),
        }
    }
}

fn path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".crab").join(FILE))
}

impl From<Context> for Client {
    fn from(context: Context) -> Self {
        let client = Client::new(context.host);
        let client = match context.token {
            Some(token) => client.token(token),
            None => client,
        };
        match context.owner {
            Some(owner) => client.owner(owner),
            None => client,
        }
    }
}
