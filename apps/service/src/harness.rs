//! Deploying, restoring, and invoking.

use crate::Service;
use anyhow::{Context, Result, bail};
use berm::{Berm, Manifest};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::Arc};

/// A harness the service is holding: its bytes pinned, its module compiled,
/// its manifest read.
pub struct Deployed {
    pub name: String,
    /// sha256 of the ELF. Redeploying different bytes under the same name is a
    /// different harness, and this is what says so.
    pub digest: String,
    berm: Berm,
}

impl Deployed {
    pub fn manifest(&self) -> &Manifest {
        self.berm.manifest()
    }
}

impl Service {
    /// Compile `elf`, store it, and make its tools reachable under `name`.
    ///
    /// Compile, then disk, then memory — in that order throughout. Compiling
    /// first keeps a rejected image from leaving behind a file that fails again
    /// on every restart; writing before publishing keeps the service from
    /// serving a tool that would vanish when it restarts.
    pub async fn deploy(&self, name: &str, elf: Vec<u8>) -> Result<Arc<Deployed>> {
        validate(name)?;
        let elf = Arc::new(elf);
        let deployed = self.compile(name.to_owned(), elf.clone()).await?;

        tokio::fs::create_dir_all(self.images())
            .await
            .context("failed to open the image directory")?;
        tokio::fs::write(self.image(name), elf.as_slice())
            .await
            .context("failed to store the image")?;

        self.publish(deployed.clone()).await;
        Ok(deployed)
    }

    /// Forget a harness and delete its image. `false` if it wasn't deployed.
    pub async fn undeploy(&self, name: &str) -> Result<bool> {
        if self.get(name).await.is_none() {
            return Ok(false);
        }
        tokio::fs::remove_file(self.image(name))
            .await
            .context("failed to remove the image")?;

        self.deployed.write().await.remove(name);
        let _ = self.changed.send(());
        Ok(true)
    }

    /// Run one tool.
    ///
    /// The outer `Result` is the service's — no such harness, no such tool, a
    /// trap. The inner one is the harness's own reported failure, which is a
    /// tool result rather than an error.
    pub async fn call(
        &self,
        harness: &str,
        tool: &str,
        args: Vec<u8>,
    ) -> Result<Result<String, String>> {
        let Some(deployed) = self.get(harness).await else {
            bail!("no harness named {harness:?} is deployed");
        };
        let tool = tool.to_owned();
        // Entering a guest blocks the thread until the guest returns, which is
        // not something a runtime worker can afford to do.
        tokio::task::spawn_blocking(move || deployed.berm.call(&tool, args))
            .await
            .context("invocation panicked")?
    }

    /// Bring back what was deployed before this process started.
    pub(crate) async fn restore(&self) -> Result<()> {
        let images = self.images();
        if !images.is_dir() {
            return Ok(());
        }

        let mut entries = tokio::fs::read_dir(&images)
            .await
            .context("failed to read the image directory")?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let Some(name) = path
                .extension()
                .filter(|extension| *extension == "elf")
                .and_then(|_| path.file_stem())
                .and_then(|stem| stem.to_str())
            else {
                continue;
            };

            let elf = Arc::new(tokio::fs::read(&path).await?);
            // One unloadable image is not a reason to come up with none of
            // them: it is reported and skipped, and the rest still serve.
            match self.compile(name.to_owned(), elf).await {
                Ok(deployed) => {
                    tracing::info!(name, digest = %deployed.digest, "restored");
                    self.publish(deployed).await;
                }
                Err(error) => tracing::error!(name, "{error:#}"),
            }
        }
        Ok(())
    }

    /// Compile an image and read what it claims to be.
    ///
    /// Doing this at deploy rather than on first call means a broken image is
    /// refused by the deploy that introduced it, not on a model's turn — and
    /// `Berm::load` checks the manifest against the symbol table on the way.
    async fn compile(&self, name: String, elf: Arc<Vec<u8>>) -> Result<Arc<Deployed>> {
        let digest = format!("{:x}", Sha256::digest(elf.as_slice()));
        let engine = self.engine.clone();
        let berm = tokio::task::spawn_blocking(move || Berm::load(&engine, &elf, &[]))
            .await
            .context("compilation panicked")??;
        Ok(Arc::new(Deployed { name, digest, berm }))
    }

    /// Make a compiled harness reachable, and tell every session the tool set
    /// moved under it.
    async fn publish(&self, deployed: Arc<Deployed>) {
        self.deployed
            .write()
            .await
            .insert(deployed.name.clone(), deployed);
        let _ = self.changed.send(());
    }

    fn images(&self) -> PathBuf {
        self.root.join("harnesses")
    }

    fn image(&self, name: &str) -> PathBuf {
        self.images().join(format!("{name}.elf"))
    }
}

/// A name has to survive being half of an MCP tool name and all of a filename.
/// The dot is the one that matters: dispatch splits `{harness}.{tool}` on the
/// first one, so a harness carrying a dot would eat its own tool.
fn validate(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("a harness name cannot be empty");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        bail!("harness name {name:?} is not lowercase letters, digits, `-` or `_`");
    }
    Ok(())
}
