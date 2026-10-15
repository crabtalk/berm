//! What a host's daemons and peers send it, and what it answers with.

use crate::Meter;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Header carrying a tool call's [`Meter`], summed over every
/// program the call reached, as JSON.
pub const METER: &str = "berm-meter";

/// Header carrying how many calls a [`CallPage`]'s filters match, across
/// every page.
pub const TOTAL: &str = "x-total-count";

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
    /// Deploy even when it breaks what callers used, or undoes a cron its
    /// owner set.
    #[serde(default)]
    pub force: bool,
    /// The cron expression of each cron tool, as the deploy's config sets
    /// them; a cron tool it leaves out has none. `None` leaves each as its
    /// owner set it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crons: Option<BTreeMap<String, String>>,
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
    /// Set when the chain is dry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dry: Option<crate::Dry>,
}

/// How a call between hosts ended, when the called service ran.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    pub result: Result<String, String>,
    /// The service refused its caller, through `berm.deny`.
    pub denied: bool,
    /// What the call used, by each service it reached.
    #[serde(default)]
    pub meters: BTreeMap<String, Meter>,
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

/// The calls of a service's tools from outside its scope that ended `done`:
/// from an account, an app, or anonymously.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Served {
    /// Each of the last 30 days with any, oldest first.
    pub days: Vec<ServedDay>,
    /// Since the service was first deployed.
    pub total: u64,
}

/// One day of [`Served`].
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServedDay {
    /// Days since the Unix epoch.
    pub day: i32,
    pub calls: u64,
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

/// `?topic=&call=&after=&page=&limit=` on a service's logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    /// Only logs carrying this topic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    /// Only logs emitted in this call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<i64>,
    /// Only logs after this sequence number.
    #[serde(default)]
    pub after: i64,
    /// Which run of `limit` logs, counting from 1.
    #[serde(default = "first")]
    pub page: i64,
    #[serde(default = "limit")]
    pub limit: i64,
}

fn first() -> i64 {
    1
}

fn limit() -> i64 {
    100
}

impl Page {
    /// The `Link` header for this page of `total` logs.
    pub fn link(&self, total: i64) -> Option<String> {
        link(self.page, self.limit, total, |page| {
            serde_urlencoded::to_string(Page {
                page,
                ..self.clone()
            })
        })
    }
}

/// `?tool=&event=&outcome=&since=&until=&page=&limit=` on a service's calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallPage {
    /// Only calls of this tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Only calls that emitted a log carrying this topic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// Only calls that ended so: `done`, `failed`, `denied` or `refused`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    /// Only calls begun at or after this, in epoch milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
    /// Only calls begun before this, in epoch milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<u64>,
    /// Which run of `limit` calls, counting from 1.
    #[serde(default = "first")]
    pub page: i64,
    #[serde(default = "limit")]
    pub limit: i64,
}

impl CallPage {
    /// The `Link` header for this page of `total` calls.
    pub fn link(&self, total: i64) -> Option<String> {
        link(self.page, self.limit, total, |page| {
            serde_urlencoded::to_string(CallPage {
                page,
                ..self.clone()
            })
        })
    }
}

/// A `Link` header in GitHub's form for page `page` of `total` items, `limit`
/// a page: `first` and `prev` past the first page, `next` and `last` before
/// the last, each a reference relative to the request whose query `query`
/// writes. `None` for the only page.
fn link(
    page: i64,
    limit: i64,
    total: i64,
    query: impl Fn(i64) -> Result<String, serde_urlencoded::ser::Error>,
) -> Option<String> {
    let last = ((total + limit - 1) / limit).max(1);
    let mut rels = Vec::new();
    if page > 1 {
        rels.push((1, "first"));
        rels.push(((page - 1).min(last), "prev"));
    }
    if page < last {
        rels.push((page + 1, "next"));
        rels.push((last, "last"));
    }
    if rels.is_empty() {
        return None;
    }
    let link = rels
        .into_iter()
        .map(|(page, rel)| format!("<?{}>; rel=\"{rel}\"", query(page).unwrap_or_default()))
        .collect::<Vec<_>>()
        .join(", ");
    Some(link)
}

/// A service's cron tools.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Crons {
    pub crons: Vec<CronJob>,
    /// Its scope is held: none runs until it is let go.
    pub held: bool,
}

/// A cron tool: one its manifest marks `#[cron]`.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub tool: String,
    /// Its expression, in UTC; `None` while it has none and is not called.
    pub cron: Option<String>,
    /// When it next runs, in epoch milliseconds; `None` while it has no
    /// expression.
    pub due: Option<u64>,
    /// Times it failed in a row.
    pub failures: u64,
    /// Its last run, in the last 30 days.
    pub last: Option<Call>,
}

/// The wakes armed to call a service's tools that have not run.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wakes {
    /// Soonest first.
    pub wakes: Vec<Wake>,
    /// Its scope is held: none runs until it is let go.
    pub held: bool,
}

/// A wake armed with `berm.call.after`.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wake {
    /// The tool it calls.
    pub tool: String,
    /// The arguments it calls the tool with, as JSON text.
    pub args: String,
    /// When it fires, in epoch milliseconds.
    pub due: u64,
    /// The service that armed it, by its name.
    pub by: String,
    /// Times it failed in a row since it was last armed.
    pub failures: u64,
    /// The last time a wake armed by the same service ran this tool, in the
    /// last 30 days.
    pub last: Option<Call>,
}

/// One run of a service: a call from outside or a wake.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Call {
    /// Its position among the service's calls; newer calls have larger ids.
    pub id: u64,
    /// When it began, in epoch milliseconds.
    pub at: u64,
    pub tool: String,
    /// As [`Usage::source`] names it.
    pub source: String,
    /// As [`Usage::caller`] names it.
    pub caller: String,
    /// As [`Usage::outcome`] names it.
    pub outcome: String,
    /// How long it ran, in milliseconds.
    pub ms: u64,
    /// What it used, summed over every service it reached. `None` for a call
    /// the host refused.
    pub meter: Option<Meter>,
    /// What its caller paid, in USD micros.
    pub price_micros: u64,
    /// How many logs it emitted.
    pub logs: u64,
}

/// One day of a service's meter.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metered {
    /// Days since the Unix epoch.
    pub day: i32,
    /// Invocations of the service: from outside, from other services, and
    /// wakes.
    pub calls: u64,
    /// Wasm instructions those invocations executed.
    pub instructions: u64,
    /// The instructions each of them executed past its first
    /// [`INCLUDED_INSTRUCTIONS`](crate::INCLUDED_INSTRUCTIONS), summed.
    #[serde(default)]
    pub overage: u64,
    /// The largest linear memory one of them reached, in bytes.
    pub memory: u64,
    /// Rows read and written in the service's database and state.
    pub read: u64,
    pub written: u64,
    /// Bytes sent and received through `berm.http.proxy`.
    pub proxied: u64,
    /// Bytes of the logs those invocations emitted.
    #[serde(default)]
    pub logged: u64,
    /// Bytes the service stored when the day was sampled: its database file
    /// and state.
    pub stored: u64,
}
