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
//! Read at every lookup, so an edit applies to the next request. bermd starts
//! no chain with a caller, so a grant is the program's whatever starts it.

use anyhow::{Context, Result};
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
type Grants = BTreeMap<String, BTreeMap<String, Grant>>;

/// Where the grants live.
#[derive(Clone)]
pub(crate) struct Secrets {
    path: PathBuf,
}

impl Secrets {
    pub(crate) fn open(root: &Path) -> Self {
        Self {
            path: root.join(FILE),
        }
    }

    /// The value of `name` granted to `program` for `host`, if any.
    pub(crate) fn grant(&self, program: &str, name: &str, host: &str) -> Result<Option<String>> {
        let mut grants = self.read()?;
        Ok(grants
            .remove(program)
            .and_then(|mut granted| granted.remove(name))
            .filter(|grant| {
                grant
                    .hosts
                    .iter()
                    .any(|allowed| allowed.eq_ignore_ascii_case(host))
            })
            .map(|grant| grant.value))
    }

    /// The names granted to `program`, for whatever hosts.
    pub(crate) fn names(&self, program: &str) -> Result<Vec<String>> {
        Ok(self
            .read()?
            .remove(program)
            .map(|granted| granted.into_keys().collect())
            .unwrap_or_default())
    }

    /// Every grant, or none when the file is absent.
    fn read(&self) -> Result<Grants> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Grants::new());
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to read {}", self.path.display()));
            }
        };
        toml::from_str(&text).with_context(|| format!("{} is not valid", self.path.display()))
    }
}
