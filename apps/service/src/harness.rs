//! Deploying, restoring, and invoking.

use crate::Service;
use anyhow::{Context, Result, bail};
use berm::Harness;
use std::sync::Arc;

impl Service {
    /// Compile `elf`, store it, and make its tools reachable under `name`.
    ///
    /// Compile, then store, then publish — berm does the first two in that
    /// order, so a rejected image leaves no record that would fail again on
    /// every restart, and a tool that is served is one a restart brings back.
    pub async fn deploy(&self, name: &str, elf: Vec<u8>) -> Result<Arc<Harness>> {
        validate(name)?;
        let harness = self.compile(name.to_owned(), Arc::new(elf)).await?;

        // Said rather than refused: a harness deployed before the one it
        // calls is ordinary, and the call reports it again if it stays that
        // way.
        let unresolved = self.unresolved(harness.manifest());
        if !unresolved.is_empty() {
            tracing::warn!(name, "nothing here answers to {}", unresolved.join(", "));
        }

        let _ = self.changed.send(());
        Ok(harness)
    }

    /// Forget a harness and drop its image. `false` if it wasn't deployed.
    pub async fn undeploy(&self, name: &str) -> Result<bool> {
        // A wake it armed has nowhere left to run. One pointed *at* it by
        // another harness stays: that slot is the armer's, and a target that
        // has gone away is reported when it fires.
        self.forget_wake(name);
        if !self.berm.remove(name)? {
            return Ok(false);
        }
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

    /// Compile an image into the runtime, off the async workers.
    async fn compile(&self, name: String, elf: Arc<Vec<u8>>) -> Result<Arc<Harness>> {
        let berm = self.berm.clone();
        tokio::task::spawn_blocking(move || berm.deploy(&name, &elf))
            .await
            .context("compilation panicked")?
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
