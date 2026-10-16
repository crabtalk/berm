//! The secrets a program may have substituted into its requests.
//!
//! Granted in `{root}/secrets.toml`, by program and then by secret name, each
//! beside the hosts it may be sent to:
//!
//! ```toml
//! [fixture.fixture]
//! value = "..."
//! hosts = ["127.0.0.1"]
//! ```
//!
//! Read at every lookup, so an edit applies to the next request. A grant is
//! the program's whatever starts it.

use anyhow::{Context, Result};
use berm_host::Secrets;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// What the file is called under the service's root.
const FILE: &str = "secrets.toml";

/// One secret granted to one program.
#[derive(Deserialize)]
struct Grant {
    value: String,
    /// The hosts it may be sent to.
    hosts: Vec<String>,
}

/// Every grant, by program and then by secret name.
type Table = BTreeMap<String, BTreeMap<String, Grant>>;

/// Where the grants live.
#[derive(Clone)]
pub struct Grants {
    path: PathBuf,
}

impl Grants {
    pub fn open(root: &Path) -> Self {
        Self {
            path: root.join(FILE),
        }
    }

    /// Every grant, or none when the file is absent.
    fn read(&self) -> Result<Table> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Table::new());
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to read {}", self.path.display()));
            }
        };
        toml::from_str(&text).with_context(|| format!("{} is not valid", self.path.display()))
    }
}

impl Secrets for Grants {
    async fn grant(&self, program: &str, secret: &str, host: &str) -> Result<Option<String>> {
        Ok(self
            .read()?
            .remove(program)
            .and_then(|mut granted| granted.remove(secret))
            .filter(|grant| {
                grant
                    .hosts
                    .iter()
                    .any(|allowed| allowed.eq_ignore_ascii_case(host))
            })
            .map(|grant| grant.value))
    }

    async fn names(&self, program: &str) -> Result<Vec<String>> {
        Ok(self
            .read()?
            .remove(program)
            .map(|granted| granted.into_keys().collect())
            .unwrap_or_default())
    }
}
