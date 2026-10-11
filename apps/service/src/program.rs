//! Deploying, removing, and invoking, as the local owner and account.

use crate::{Deployed, LOCAL, Service};
use axum::http::StatusCode;
use berm_api::{
    Break,
    host::{Caller, Deploy},
};
use berm_host::{Denied, deny};

impl Service {
    /// Make the tools of `image` reachable under `name`, answering with how it
    /// breaks callers of the program it replaced. Refused as the host refuses
    /// a deploy: a tool made public that `public` does not confirm, or, unless
    /// `force`, a break of what a caller used.
    pub async fn deploy(
        &self,
        name: &str,
        image: Vec<u8>,
        public: &[String],
        force: bool,
    ) -> Result<(Deployed, Vec<Break>), Denied> {
        validate(name)?;
        let replaced = self.get(name);
        let digest = self.images.put(&image)?;
        let program = self
            .host
            .deploy(
                LOCAL,
                name,
                Deploy {
                    reference: digest.clone(),
                    digest,
                    public: public.to_vec(),
                    force,
                },
            )
            .await?;
        let breaks = replaced
            .map(|replaced| replaced.manifest().breaks(program.manifest()))
            .unwrap_or_default();
        for broken in &breaks {
            tracing::warn!(name, "breaks callers: {broken}");
        }
        let deployed = Deployed {
            name: name.into(),
            program,
        };
        let unresolved = self.unresolved(&deployed);
        if !unresolved.is_empty() {
            tracing::warn!(name, "nothing here answers to {unresolved}");
        }
        let _ = self.changed.send(());
        Ok((deployed, breaks))
    }

    /// Stop running a program and drop what it committed. `false` if it wasn't
    /// deployed.
    pub async fn undeploy(&self, name: &str) -> Result<bool, Denied> {
        let removed = self.host.remove(LOCAL, name).await?;
        if removed {
            let _ = self.changed.send(());
        }
        Ok(removed)
    }

    /// Run one public tool as the local account. The inner `Result` is the
    /// program's own report.
    pub async fn call(
        &self,
        name: &str,
        tool: &str,
        args: Vec<u8>,
    ) -> Result<Result<String, String>, Denied> {
        let (_, output) = self
            .host
            .call(LOCAL, name, &caller(), tool, args, false)
            .await?;
        Ok(output.into())
    }
}

/// The local account.
pub(crate) fn caller() -> Caller {
    Caller {
        id: LOCAL.to_owned(),
        app: None,
        user: None,
        client: None,
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
