//! The edges a daemon gives its [`Host`](crate::Host): what differs between
//! running services on a laptop and on a fleet's node.

use crate::Denied;
use anyhow::Result;
use berm::syscall::http::{Request, Response};
use berm_api::{
    Logged,
    host::{Caller, Link, Reply, Used},
};
use std::future::Future;

/// One daemon's edges, as types.
pub trait Env: Send + Sync + 'static {
    /// Sends `berm.http.fetch`, and `berm.http.proxy` through a proxy. Follows
    /// no redirects; the host bounds each request by its call's deadline.
    type Fetch: tower::Service<Request, Response = Response, Error = anyhow::Error, Future: Send + 'static>
        + Clone
        + Send
        + Sync
        + 'static;
    type Images: Images;
    type Peers: Peers;
    type Secrets: Secrets;
    type Admit: Admit;
    type Clock: Clock;
}

/// One daemon's edges, as values.
pub struct Edges<E: Env> {
    pub fetch: E::Fetch,
    /// What `berm.http.proxy` is sent through; without one, a manifest that
    /// declares it is refused at deploy.
    pub proxy: Option<E::Fetch>,
    pub images: E::Images,
    pub peers: E::Peers,
    pub secrets: E::Secrets,
    pub admit: E::Admit,
    /// What `berm.now` answers, when a wake is due, and the day a use or a
    /// meter is counted on.
    pub clock: E::Clock,
}

/// A program's bytes for a deploy.
pub trait Images: Send + Sync + 'static {
    /// The image `digest` names, published as `reference`.
    fn pull(&self, reference: &str, digest: &str) -> impl Future<Output = Result<Vec<u8>>> + Send;
}

/// The hosts running the services this one does not, each named by the peer a
/// [`Route`](berm_api::host::Route) gives.
pub trait Peers: Send + Sync + 'static {
    /// Call `tool` of `service` on `peer` as `link`. A refusal there is a
    /// [`berm::Refused`] here.
    fn link(
        &self,
        peer: &str,
        service: &str,
        tool: &str,
        args: Vec<u8>,
        link: Link,
    ) -> impl Future<Output = Result<Reply>> + Send;

    /// `service`'s logs on `peer` after `after`, at most `limit`.
    fn logs(
        &self,
        peer: &str,
        service: &str,
        after: i64,
        limit: i64,
    ) -> impl Future<Output = Result<Vec<Logged>>> + Send;

    /// Hand `peer` the uses counted here of services it runs.
    fn ship(&self, peer: &str, used: &[Used]) -> impl Future<Output = Result<()>> + Send;
}

/// What `{secret:NAME}` in a fetch header stands for.
pub trait Secrets: Send + Sync + 'static {
    /// The value of `secret` granted to `service` for `host`, if any.
    fn grant(
        &self,
        service: &str,
        secret: &str,
        host: &str,
    ) -> impl Future<Output = Result<Option<String>>> + Send;
}

/// Who may call a public tool from outside. Asked before the tool runs, once
/// the tool is public and its manifest takes calls from outside; a refusal is
/// answered as it is and counted as `refused`.
pub trait Admit: Send + Sync + 'static {
    fn admit(
        &self,
        service: &str,
        tool: &str,
        query: bool,
        caller: Option<&Caller>,
    ) -> impl Future<Output = Result<(), Denied>> + Send;
}

/// What time it is.
pub trait Clock: Send + Sync + 'static {
    /// Milliseconds since the Unix epoch. Called inside a syscall, where an
    /// unwind aborts the process.
    fn now(&self) -> u64;
}

/// The system's clock.
pub struct System;

impl Clock for System {
    fn now(&self) -> u64 {
        berm::syscall::call::system_now()
    }
}
