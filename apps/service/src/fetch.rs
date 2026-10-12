//! `berm.http.fetch` and `berm.http.proxy`, sent from this process.

use anyhow::{Context, Result, bail};
use berm::syscall::http::{Request, Response};
use std::{
    future::Future,
    pin::Pin,
    task::{self, Poll},
};

/// The largest body a response may carry. The guest reads it into its own
/// heap.
const MAX_BODY: usize = 16 << 20;

/// A client that answers a redirect to the guest as one.
#[derive(Clone)]
pub struct Fetch(reqwest::Client);

impl Fetch {
    /// Through `proxy` when given, with no idle connection kept, so a rotating
    /// proxy gives each request a fresh exit.
    pub fn new(proxy: Option<&str>) -> Result<Self> {
        let mut client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none());
        if let Some(proxy) = proxy {
            client = client
                .proxy(reqwest::Proxy::all(proxy).context("the proxy URL is not one")?)
                .pool_max_idle_per_host(0);
        }
        let client = client.build().context("failed to build the HTTP client")?;
        Ok(Self(client))
    }
}

impl tower::Service<Request> for Fetch {
    type Response = Response;
    type Error = anyhow::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Response>> + Send>>;

    fn poll_ready(&mut self, _: &mut task::Context<'_>) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let client = self.0.clone();
        Box::pin(async move {
            let method = reqwest::Method::from_bytes(request.method.as_bytes())
                .with_context(|| format!("{:?} is not an HTTP method", request.method))?;
            let mut sending = client.request(method, &request.url).body(request.body);
            for (name, value) in &request.headers {
                sending = sending.header(name, value);
            }
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
        })
    }
}
