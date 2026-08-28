//! Deploying, restoring, and invoking.

use crate::Service;
use anyhow::{Context, Result, bail};
use berm::Harness;
use std::{path::PathBuf, sync::Arc};

impl Service {
    /// Compile `elf`, store it, and make its tools reachable under `name`.
    ///
    /// Compile, then disk, then publish — in that order throughout. Compiling
    /// first keeps a rejected image from leaving behind a file that fails again
    /// on every restart; writing before announcing keeps the service from
    /// serving a tool that would vanish when it restarts.
    pub async fn deploy(&self, name: &str, elf: Vec<u8>) -> Result<Arc<Harness>> {
        validate(name)?;
        let harness = self.compile(name.to_owned(), Arc::new(elf.clone())).await?;

        tokio::fs::create_dir_all(self.images())
            .await
            .context("failed to open the image directory")?;
        tokio::fs::write(self.image(name), &elf)
            .await
            .context("failed to store the image")?;

        let _ = self.changed.send(());
        Ok(harness)
    }

    /// Forget a harness and delete its image. `false` if it wasn't deployed.
    pub async fn undeploy(&self, name: &str) -> Result<bool> {
        if self.get(name).is_none() {
            return Ok(false);
        }
        tokio::fs::remove_file(self.image(name))
            .await
            .context("failed to remove the image")?;

        // A wake it armed has nowhere left to run. One pointed *at* it by
        // another harness stays: that slot is the armer's, and a target that
        // has gone away is reported when it fires.
        self.forget_wake(name);
        self.berm.remove(name);
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
        let Some(harness) = self.get(harness) else {
            bail!("no harness named {harness:?} is deployed");
        };
        let tool = tool.to_owned();
        // Entering a guest blocks the thread until the guest returns, which is
        // not something a runtime worker can afford to do.
        tokio::task::spawn_blocking(move || harness.call(&tool, args))
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
                Ok(harness) => tracing::info!(name, digest = %harness.digest, "restored"),
                Err(error) => tracing::error!(name, "{error:#}"),
            }
        }
        Ok(())
    }

    /// Compile an image into the runtime, off the async workers.
    async fn compile(&self, name: String, elf: Arc<Vec<u8>>) -> Result<Arc<Harness>> {
        let berm = self.berm.clone();
        tokio::task::spawn_blocking(move || berm.deploy(&name, &elf))
            .await
            .context("compilation panicked")?
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
