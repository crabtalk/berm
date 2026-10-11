//! `berm.http.fetch`, sent through the daemon's [`Env::Fetch`].

use crate::env::{Env, Secrets};
use anyhow::Context;
use berm::{
    Callsite, Syscall,
    syscall::http::{self, Request},
};
use std::sync::Arc;
use tower::ServiceExt;

/// The syscall, each request given up at its call's deadline.
pub(crate) fn syscall<E: Env>(fetch: E::Fetch, secrets: Arc<E::Secrets>) -> Syscall {
    http::fetch(
        move |at: Callsite, request: Request| {
            let fetch = fetch.clone();
            async move {
                let host = request.host.clone();
                tokio::time::timeout_at(at.deadline.into(), fetch.oneshot(request))
                    .await
                    .with_context(|| format!("{host} did not answer before the call's deadline"))?
            }
        },
        move |at: Callsite, secret: String, to: String| {
            let secrets = secrets.clone();
            async move { secrets.grant(&at.program, &secret, &to).await }
        },
    )
}
