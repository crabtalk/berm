//! Deploying, removing, and invoking.

use crate::Service;
use axum::http::StatusCode;
use berm::Program;
use berm_api::{Break, host::Deploy};
use berm_host::{Denied, deny};
use std::{collections::BTreeMap, sync::Arc};

impl Service {
    /// Make the tools of `image` reachable under `name`, answering with how it
    /// breaks callers of the program it replaced. Refused as the host refuses
    /// a deploy: a tool made public that `public` does not confirm, or, unless
    /// `force`, a break of what a caller used or a cron its owner set that
    /// `crons` undoes.
    pub async fn deploy(
        &self,
        name: &str,
        image: Vec<u8>,
        public: &[String],
        force: bool,
        crons: Option<BTreeMap<String, String>>,
    ) -> Result<(Arc<Program>, Vec<Break>), Denied> {
        validate(name)?;
        let replaced = self.get(name);
        let digest = self.images.put(&image)?;
        let program = self
            .host
            .deploy(
                name,
                Deploy {
                    reference: digest.clone(),
                    digest,
                    public: public.to_vec(),
                    force,
                    crons,
                },
            )
            .await?;
        let breaks = replaced
            .map(|replaced| replaced.manifest().breaks(program.manifest()))
            .unwrap_or_default();
        for broken in &breaks {
            tracing::warn!(name, "breaks callers: {broken}");
        }
        let unresolved = self.host.runtime(name).await?.unresolved;
        if !unresolved.is_empty() {
            tracing::warn!(name, "nothing here answers to {unresolved}");
        }
        let _ = self.changed.send(());
        Ok((program, breaks))
    }

    /// Stop running a program and drop what it committed. `false` if it wasn't
    /// deployed.
    pub async fn undeploy(&self, name: &str) -> Result<bool, Denied> {
        let removed = self.host.remove(name).await?;
        if removed {
            let _ = self.changed.send(());
        }
        Ok(removed)
    }

    /// Run one public tool with no caller. The inner `Result` is the program's
    /// own report.
    pub async fn call(
        &self,
        name: &str,
        tool: &str,
        args: Vec<u8>,
    ) -> Result<Result<String, String>, Denied> {
        let (_, output) = self.host.call(name, None, tool, args, false).await?;
        Ok(output.into())
    }
}

/// A name has to survive being half of an MCP tool name. The dot is the one
/// that matters: dispatch splits `{program}.{tool}` on the first one, so a
/// program carrying a dot would eat its own tool.
fn validate(name: &str) -> Result<(), Denied> {
    if name.is_empty() {
        return Err(deny(
            StatusCode::BAD_REQUEST,
            "a program name cannot be empty",
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(deny(
            StatusCode::BAD_REQUEST,
            format!("program name {name:?} is not lowercase letters, digits, `-` or `_`"),
        ));
    }
    Ok(())
}
