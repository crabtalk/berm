//! `berm.http.fetch`, sent straight from this process.
//!
//! berm has already refused any host the program's manifest does not declare.
//! bermd holds no secrets, so a request naming one is refused too.

use anyhow::{Context, Result, bail};
use berm::{
    Callsite, Syscall,
    syscall::http::{self, Request, Response},
};
use std::time::Instant;

/// The largest body a response may carry. The guest reads it into its own
/// heap.
const MAX_BODY: usize = 16 << 20;

pub(crate) fn syscall() -> Result<Syscall> {
    // A redirect is answered to the guest as one, so a host the manifest
    // declares cannot hand the request on to one it does not.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("failed to build the HTTP client")?;
    Ok(http::fetch(
        move |at: Callsite, request: Request| {
            let client = client.clone();
            async move { send(&client, at.deadline, request).await }
        },
        |_: Callsite, _: String, _: String| std::future::ready(Ok(None)),
    ))
}

/// Send `request`, giving up at `deadline`.
async fn send(client: &reqwest::Client, deadline: Instant, request: Request) -> Result<Response> {
    let method = reqwest::Method::from_bytes(request.method.as_bytes())
        .with_context(|| format!("{:?} is not an HTTP method", request.method))?;
    let mut sending = client.request(method, &request.url).body(request.body);
    for (name, value) in &request.headers {
        sending = sending.header(name, value);
    }

    let answered = async {
        let mut response = sending.send().await?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.to_string(),
                    String::from_utf8_lossy(value.as_bytes()).into_owned(),
                )
            })
            .collect();
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if body.len() + chunk.len() > MAX_BODY {
                bail!("{} answered with more than {MAX_BODY} bytes", request.host);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Response {
            status,
            headers,
            body,
        })
    };
    tokio::time::timeout_at(deadline.into(), answered)
        .await
        .with_context(|| format!("{} did not answer before the call's deadline", request.host))?
}
