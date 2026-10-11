//! What a host's daemons and peers send it, and what it answers with.

use serde::{Deserialize, Serialize};

/// Header carrying the [`Caller`] a call to bermd is made as, as JSON. bermd
/// verifies none of it.
pub const CALLER: &str = "x-berm-caller";

/// Who called from outside, as the daemon hands them to the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Caller {
    /// The account calling, or for an app's call, the account owning the app.
    pub id: String,
    /// The app a browser called through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    /// The end user the call is made for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    /// For an app's anonymous call, the client's address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
}

/// An end user, as an app's identity provider named them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub issuer: String,
    pub subject: String,
}

/// A deploy: the image `digest` names, published as `reference`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deploy {
    pub reference: String,
    pub digest: String,
    /// The tools this deploy may make public that the deployed version does
    /// not.
    #[serde(default)]
    pub public: Vec<String>,
    /// Deploy even when it breaks what callers used.
    #[serde(default)]
    pub force: bool,
}

/// Where a service this host does not run lives: the peer it runs on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub service: String,
    pub peer: String,
}

/// A call from a service on one host to a service on another, as the link of
/// the chain it runs as.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    /// What the called service is told called it: a service's name, or
    /// nothing for a wake.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller: Option<String>,
    /// The key of the service the call is counted against.
    pub from: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    pub depth: u32,
    pub query: bool,
    /// Milliseconds left before the chain's deadline.
    pub timeout: u64,
    /// The services the call sits under, outermost first, the called one last.
    pub stack: Vec<String>,
}

/// How a call between hosts ended, when the called service ran.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    pub result: Result<String, String>,
    /// The service refused its caller, through `berm.deny`.
    pub denied: bool,
}

/// Uses of a service counted on another host, for the host it runs on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Used {
    pub service: String,
    pub kind: String,
    pub item: String,
    pub source: String,
    pub caller: String,
    pub outcome: String,
    pub day: i32,
    pub calls: i64,
    pub last: i64,
}

/// One caller of one of a service's tools or events over the last 30 days.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    /// `tool` or `event`.
    pub kind: String,
    pub item: String,
    /// `service` (a service in the same scope), `account`, `app`, or
    /// `anonymous` for a call from outside without a caller.
    pub source: String,
    /// The service's name, or the account or app.
    pub caller: String,
    /// `done`, `failed`, `denied` (the tool refused its caller) or `refused`
    /// (the host refused it before the tool ran).
    pub outcome: String,
    pub calls: i64,
    /// The last call, in epoch seconds.
    pub last: i64,
    /// For a tool, whether the deployed version makes it public.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public: Option<bool>,
}

/// `?topic=&after=&limit=` on a service's logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    /// Only logs carrying this topic.
    #[serde(default)]
    pub topic: Option<String>,
    /// Only logs after this sequence number.
    #[serde(default)]
    pub after: i64,
    #[serde(default = "limit")]
    pub limit: i64,
}

fn limit() -> i64 {
    100
}
