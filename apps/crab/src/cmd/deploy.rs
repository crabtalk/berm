//! `crab deploy` — hand the context's host an image.

use crate::{
    Client,
    cmd::{inspect, push},
};
use anyhow::{Context, Result, bail};
use berm_api::Manifest;
use berm_index::Source;
use berm_oci::{Access, Reference, Registry};
use std::{
    fs,
    io::{self, BufRead, IsTerminal, Write},
    path::Path,
    str::FromStr,
};

/// Deploy `image` as `name`. A tool it makes public that the deployed program
/// does not is asked about, unless listed in `public`. On Cloud, a file is
/// pushed to `reference` first, and the reference listed in the context's
/// index before it is deployed.
pub fn run(
    client: &Client,
    name: &str,
    image: &str,
    reference: Option<&str>,
    public: &[String],
    force: bool,
) -> Result<()> {
    let bytes = read(image)?;
    let manifest = Manifest::from_image(&bytes)?;
    let confirmed = confirm(client, name, &manifest, public)?;
    let program = match &client.owner {
        None => client.deploy(name, bytes, &confirmed, force)?,
        Some(_) => {
            let reference = match Path::new(image).exists() {
                true => {
                    let reference = reference.context(
                        "Cloud deploys a file through a registry; name where with `--reference`",
                    )?;
                    push::push(reference, &bytes)?;
                    reference
                }
                false => image,
            };
            Source::new(Some(&client.host))?.publish(reference, client.token.as_deref())?;
            client.deploy_reference(name, reference, &confirmed, force)?
        }
    };
    // The host compiled it to answer, so what comes back is what it will
    // serve — worth showing rather than saying "ok".
    inspect::show(&program, &client.terms(name)?);
    Ok(())
}

/// `public`, and each tool `manifest` makes public that the deployed program
/// does not, once the operator confirms it.
fn confirm(
    client: &Client,
    name: &str,
    manifest: &Manifest,
    public: &[String],
) -> Result<Vec<String>> {
    let deployed: Vec<String> = client
        .inspect(name)
        .map(|program| {
            program
                .tools
                .into_iter()
                .filter(|tool| tool.public)
                .map(|tool| tool.name)
                .collect()
        })
        .unwrap_or_default();
    let mut confirmed = public.to_vec();
    for tool in manifest.tools.iter().filter(|tool| tool.public) {
        if deployed.contains(&tool.name) || confirmed.contains(&tool.name) {
            continue;
        }
        if !io::stdin().is_terminal() {
            bail!(
                "{} would become public; pass `--public {}` to confirm",
                tool.name,
                tool.name
            );
        }
        print!(
            "{name}.{} becomes public: any caller may run it, with arguments of their choosing. Confirm? [y/N] ",
            tool.name
        );
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().lock().read_line(&mut answer)?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            bail!("not deployed: {} was not confirmed", tool.name);
        }
        confirmed.push(tool.name.clone());
    }
    Ok(confirmed)
}

/// A file if one is there, a registry reference otherwise. The file wins, so an
/// image sitting in the working directory is never mistaken for something to go
/// and fetch.
fn read(image: &str) -> Result<Vec<u8>> {
    let path = Path::new(image);
    if path.exists() {
        return fs::read(path).with_context(|| format!("cannot read {image}"));
    }

    let reference = Reference::from_str(image)
        .with_context(|| format!("{image:?} is neither a file nor a registry reference"))?;
    Registry::open(&reference, Access::Read)?.pull(&reference.reference)
}
