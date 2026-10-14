//! `crab context` — name the hosts `crab` talks to, and pick one.

use crate::context::{self, Context, Contexts};
use anyhow::{Result, bail};
use crabtalk_sdk::client::Owner;

/// Name `host` as `name`: a bermd, or with `owner`, Cloud.
pub fn add(name: &str, host: &str, token: Option<&str>, owner: Option<Owner>) -> Result<()> {
    if name == context::DEFAULT {
        bail!(
            "{:?} is this machine's bermd and cannot be replaced",
            context::DEFAULT
        );
    }
    if owner.is_some() != token.is_some() {
        bail!("a Cloud context takes both `--token` and `--org` or `--user`");
    }
    let mut contexts = Contexts::load()?;
    contexts.contexts.insert(
        name.to_owned(),
        Context {
            host: host.to_owned(),
            token: token.map(str::to_owned),
            owner,
        },
    );
    contexts.save()?;
    println!("added {name}");
    Ok(())
}

/// Send every later call to `name`.
pub fn use_(name: &str) -> Result<()> {
    let mut contexts = Contexts::load()?;
    contexts.get(Some(name))?;
    contexts.current = Some(name.to_owned());
    contexts.save()?;
    println!("using {name}");
    Ok(())
}

pub fn ls() -> Result<()> {
    let contexts = Contexts::load()?;
    let current = contexts.current.as_deref().unwrap_or(context::DEFAULT);
    let default = contexts.get(Some(context::DEFAULT))?;
    let rows = std::iter::once((context::DEFAULT, &default)).chain(
        contexts
            .contexts
            .iter()
            .map(|(name, at)| (name.as_str(), at)),
    );
    for (name, at) in rows {
        let mark = if name == current { "*" } else { " " };
        match &at.owner {
            Some(Owner::Org(org)) => println!("{mark} {name}  {}  org {org}", at.host),
            Some(Owner::User(user)) => println!("{mark} {name}  {}  user {user}", at.host),
            None => println!("{mark} {name}  {}", at.host),
        }
    }
    Ok(())
}

pub fn rm(name: &str) -> Result<()> {
    let mut contexts = Contexts::load()?;
    if contexts.contexts.remove(name).is_none() {
        bail!("no context named {name:?}");
    }
    if contexts.current.as_deref() == Some(name) {
        contexts.current = None;
    }
    contexts.save()?;
    println!("removed {name}");
    Ok(())
}
