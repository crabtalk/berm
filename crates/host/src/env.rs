//! The edges a daemon gives its [`Host`](crate::Host): what differs between
//! running services on a laptop and on a fleet's node.

use crate::Layers;
use anyhow::Result;
use berm::{
    Meter,
    syscall::http::{Request, Response},
};
use berm_api::host::{Link, Reply, Used};
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
    type Observe: Observe;
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
    /// Put around every call from outside, outermost first. A refusal one
    /// answers with a `4xx` is counted as `refused`.
    pub layers: Layers,
    pub observe: E::Observe,
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

    /// The names of the secrets granted to `service`, for whatever hosts.
    fn names(&self, service: &str) -> impl Future<Output = Result<Vec<String>>> + Send;
}

/// Told of each invocation this host runs once it ran, trapped or not, with
/// what it used itself, the services it called left out. Cannot change how it
/// ended.
pub trait Observe: Send + Sync + 'static {
    /// `call` is the run on this host the invocation is part of, as the
    /// service's calls number it; `None` in a dry chain or a link from a peer.
    fn invoked(&self, service: &str, tool: &str, call: Option<u64>, used: &Meter);
}

impl Observe for () {
    fn invoked(&self, _: &str, _: &str, _: Option<u64>, _: &Meter) {}
}

impl<O: Observe> Observe for std::sync::Arc<O> {
    fn invoked(&self, service: &str, tool: &str, call: Option<u64>, used: &Meter) {
        (**self).invoked(service, tool, call, used);
    }
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
