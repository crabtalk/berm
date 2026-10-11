//! The edges a daemon gives its [`Host`](crate::Host): what differs between
//! running services on a laptop and on a fleet's node.

use anyhow::Result;
use berm::syscall::http::{Request, Response};
use berm_api::{
    Logged,
    host::{Link, Reply, Used},
};
use std::future::Future;

/// One daemon's edges, as types.
pub trait Env: Send + Sync + 'static {
    /// Sends `berm.http.fetch`. Follows no redirects; the host bounds each
    /// request by its call's deadline.
    type Fetch: tower::Service<Request, Response = Response, Error = anyhow::Error, Future: Send + 'static>
        + Clone
        + Send
        + Sync
        + 'static;
    type Images: Images;
    type Peers: Peers;
    type Secrets: Secrets;
}

/// A program's bytes for a deploy.
pub trait Images: Send + Sync + 'static {
    /// The image `digest` names, published as `reference`.
    fn pull(&self, reference: &str, digest: &str) -> impl Future<Output = Result<Vec<u8>>> + Send;
}

/// The hosts running the services this one does not, each named by the peer a
/// [`Route`](berm_api::host::Route) gives.
pub trait Peers: Send + Sync + 'static {
    /// Call `tool` of `owner_id/name` on `peer` as `link`. A refusal there is a
    /// [`berm::Refused`] here.
    fn link(
        &self,
        peer: &str,
        owner_id: &str,
        name: &str,
        tool: &str,
        args: Vec<u8>,
        link: Link,
    ) -> impl Future<Output = Result<Reply>> + Send;

    /// `owner_id/name`'s logs on `peer` after `after`, at most `limit`.
    fn logs(
        &self,
        peer: &str,
        owner_id: &str,
        name: &str,
        after: i64,
        limit: i64,
    ) -> impl Future<Output = Result<Vec<Logged>>> + Send;

    /// Hand `peer` the uses counted here of services it runs.
    fn ship(&self, peer: &str, used: &[Used]) -> impl Future<Output = Result<()>> + Send;
}

/// What `{secret:NAME}` in a fetch header stands for.
pub trait Secrets: Send + Sync + 'static {
    /// The value of `secret` granted to `owner_id/name` for `host`, if any.
    fn grant(
        &self,
        owner_id: &str,
        name: &str,
        secret: &str,
        host: &str,
    ) -> impl Future<Output = Result<Option<String>>> + Send;
}
