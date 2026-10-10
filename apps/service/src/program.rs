//! Deploying, restoring, and invoking.

use crate::{Service, database, logs, store};
use anyhow::{Context, Result, bail};
use berm::Program;
use berm_api::Break;
use std::sync::Arc;

impl Service {
    /// Compile `image`, store it, and make its tools reachable under `name`,
    /// answering with how it breaks callers of the program it replaced.
    /// Refused while a tool it makes public that the program it replaces did
    /// not is missing from `public`.
    ///
    /// Compile, then store, then publish — berm does the first two in that
    /// order, so a rejected image leaves no record that would fail again on
    /// every restart, and a tool that is served is one a restart brings back.
    pub async fn deploy(
        &self,
        name: &str,
        image: Vec<u8>,
        public: &[String],
    ) -> Result<(Arc<Program>, Vec<Break>)> {
        validate(name)?;
        let replaced = self.get(name);
        let manifest = berm::Manifest::from_image(&image)?;
        let published: Vec<&str> = match &replaced {
            Some(replaced) => replaced.manifest().publishes(&manifest),
            None => manifest
                .tools
                .iter()
                .filter(|tool| tool.public)
                .map(|tool| tool.name.as_str())
                .collect(),
        };
        let unconfirmed: Vec<&str> = published
            .into_iter()
            .filter(|tool| !public.iter().any(|confirmed| confirmed == tool))
            .collect();
        if !unconfirmed.is_empty() {
            bail!(
                "this deploy makes {} public; confirm each with `public`",
                unconfirmed.join(", ")
            );
        }
        // Migrated before the image is compiled: a failed migration refuses
        // the deploy, and the program it replaces goes on serving.
        if let Some(database) = manifest.database {
            let applied = self
                .databases
                .get(name)?
                .migrate(database.migrations)
                .await?;
            if !applied.is_empty() {
                tracing::info!(name, "migrated: {}", applied.join(", "));
            }
        }
        let program = self.compile(name.to_owned(), Arc::new(image)).await?;

        // Said rather than refused, as `unresolved` is.
        let breaks = replaced
            .map(|replaced| replaced.manifest().breaks(program.manifest()))
            .unwrap_or_default();
        for broken in &breaks {
            tracing::warn!(name, "breaks callers: {broken}");
        }

        // Said rather than refused: a program deployed before the one it
        // calls is ordinary, and the call reports it again if it stays that
        // way.
        let unresolved = self.unresolved(&program);
        if !unresolved.is_empty() {
            tracing::warn!(name, "nothing here answers to {unresolved}");
        }

        let _ = self.changed.send(());
        self.deliver();
        Ok((program, breaks))
    }

    /// Forget a program and drop its image. `false` if it wasn't deployed.
    pub async fn undeploy(&self, name: &str) -> Result<bool> {
        // A wake it armed has nowhere left to run. One pointed *at* it by
        // another program stays: that slot is the armer's, and a target that
        // has gone away is reported when it fires.
        self.forget_wake(name);
        if !self.berm.remove(name)? {
            return Ok(false);
        }
        let _ = self.changed.send(());
        Ok(true)
    }

    /// Run one tool, and commit what the chain wrote and emitted.
    ///
    /// The outer `Result` is the service's — no such program, no such tool, a
    /// trap, a commit that failed. The inner one is the program's own reported
    /// failure, which is a tool result rather than an error.
    ///
    /// Logs are appended before writes land, so a crash between them repeats
    /// logs rather than losing them.
    pub async fn call(
        &self,
        program: &str,
        tool: &str,
        args: Vec<u8>,
    ) -> Result<Result<String, String>> {
        let Some(program) = self.get(program) else {
            bail!("no program named {program:?} is deployed");
        };
        // A call into a program keeping a database holds its writer and runs in
        // one transaction, committed with the program's own writes.
        let query = program
            .manifest()
            .tools
            .iter()
            .any(|spec| spec.name == tool && spec.query);
        let writing = match database::keeps(&program) && !query {
            true => Some(self.databases.get(&program.name)?.write().await?),
            false => None,
        };
        // bermd authenticates no one, so no chain it starts has a caller.
        let outcome = program.call(None, tool, args).await?;
        logs::append(&self.root, &outcome.changes.logs)?;
        let (own, rest): (Vec<_>, Vec<_>) = outcome
            .changes
            .writes
            .into_iter()
            .partition(|write| write.program == program.name);
        store::commit(&self.root, &rest)?;
        match writing {
            Some(writing) if outcome.result.is_ok() => writing.commit(own, Vec::new()).await?,
            Some(_) => {}
            None => store::commit(&self.root, &own)?,
        }
        if !outcome.changes.logs.is_empty() {
            self.deliver();
        }
        Ok(outcome.result)
    }

    /// Compile an image into the runtime, off the async workers.
    async fn compile(&self, name: String, image: Arc<Vec<u8>>) -> Result<Arc<Program>> {
        let berm = self.berm.clone();
        tokio::task::spawn_blocking(move || berm.deploy(&name, &image))
            .await
            .context("compilation panicked")?
    }
}

/// A name has to survive being half of an MCP tool name and all of a filename.
/// The dot is the one that matters: dispatch splits `{program}.{tool}` on the
/// first one, so a program carrying a dot would eat its own tool.
fn validate(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("a program name cannot be empty");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        bail!("program name {name:?} is not lowercase letters, digits, `-` or `_`");
    }
    Ok(())
}
