//! `berm deploy` — hand bermd an image.

use crate::{Client, cmd::inspect};
use anyhow::{Context, Result, bail};
use berm_api::Manifest;
use berm_oci::{Access, Reference, Registry};
use std::{
    fs,
    io::{self, BufRead, IsTerminal, Write},
    path::Path,
    str::FromStr,
};

/// Deploy `image` as `name`. A tool it makes public that the deployed program
/// does not is asked about, unless listed in `public`.
pub fn run(client: &Client, name: &str, image: &str, public: &[String]) -> Result<()> {
    let image = read(image)?;
    let manifest = Manifest::from_image(&image)?;
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
    let program = client.deploy(name, image, &confirmed)?;
    // The service compiled it to answer, so what comes back is what it will
    // serve — worth showing rather than saying "ok".
    inspect::show(&program);
    Ok(())
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
