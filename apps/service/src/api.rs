//! The control API: what a UI or a CLI drives the service with.
//!
//! Resource-shaped, on one endpoint, for the reason dockerd is: the clients are
//! a UI, a CLI and `curl`, and a program is a resource. The MCP endpoint that
//! agents reach sits on the same server at `/mcp`.

use crate::Service;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{FromRequestParts, Path, Query, State},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use berm::Program;
use berm_api::{
    Budget, DryRun, Logged, Price, Program as Wire, Terms,
    host::{self, Caller, Metered, Page},
};
use berm_host::{Denied, deny};
use rmcp::transport::{
    StreamableHttpService, streamable_http_server::session::local::LocalSessionManager,
};
use serde::Deserialize;
use std::sync::Arc;

pub fn router(service: Arc<Service>) -> Router {
    // `route_service` rather than `nest_service`: MCP Streamable HTTP is one
    // path taking POST, GET and DELETE, and nesting can drop the `Host` header
    // the transport validates against.
    let mcp = StreamableHttpService::new(
        {
            let service = service.clone();
            move || Ok(crate::mcp::Mcp::new(service.clone()))
        },
        Arc::new(LocalSessionManager::default()),
        Default::default(),
    );

    Router::new()
        .route("/programs", get(list))
        .route(
            "/programs/{name}",
            get(inspect).put(deploy).delete(undeploy),
        )
        .route("/programs/{name}/tools/{tool}", post(call).get(query))
        .route("/programs/{name}/tools/{tool}/dry", post(dry))
        .route("/programs/{name}/logs", get(logs))
        .route("/programs/{name}/meters", get(meters))
        .route("/programs/{name}/terms", get(terms))
        .route("/programs/{name}/tools/{tool}/budget", put(budget))
        .route("/programs/{name}/tools/{tool}/price", put(price))
        .route_service("/mcp", mcp)
        .with_state(service)
}

impl Service {
    /// What berm read out of the image, on the wire, and what of it this
    /// service has nothing to answer with.
    fn describe(&self, deployed: &Program) -> Wire {
        let manifest = deployed.manifest();
        Wire {
            name: deployed.name.to_string(),
            version: manifest.version.clone(),
            digest: deployed.digest.clone(),
            usage: manifest.usage.clone(),
            tools: manifest.tools.clone(),
            deps: manifest.deps.clone(),
            events: manifest.events.clone(),
            unresolved: self.unresolved(deployed),
            breaks: Vec::new(),
        }
    }
}

async fn list(State(service): State<Arc<Service>>) -> Json<Vec<Wire>> {
    Json(
        service
            .list()
            .iter()
            .map(|one| service.describe(one))
            .collect(),
    )
}

async fn inspect(
    State(service): State<Arc<Service>>,
    Path(name): Path<String>,
) -> Result<Json<Wire>, Denied> {
    match service.get(&name) {
        Some(deployed) => Ok(Json(service.describe(&deployed))),
        None => Err(missing(&name)),
    }
}

/// What a call of each public tool is held to.
async fn terms(
    State(service): State<Arc<Service>>,
    Path(name): Path<String>,
) -> Result<Json<Terms>, Denied> {
    Ok(Json(service.host.terms(&name)?))
}

/// Set what one call of a tool may use, from its next call.
async fn budget(
    State(service): State<Arc<Service>>,
    Path((name, tool)): Path<(String, String)>,
    Json(budget): Json<Budget>,
) -> Result<StatusCode, Denied> {
    service.host.set_budget(&name, &tool, budget).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Set what a call of a tool that ends `done` costs, from its next call. bermd
/// charges nothing.
async fn price(
    State(service): State<Arc<Service>>,
    Path((name, tool)): Path<(String, String)>,
    Json(Price { price_micros }): Json<Price>,
) -> Result<StatusCode, Denied> {
    service.host.set_price(&name, &tool, price_micros).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `?public=a,b&force=true` on a deploy: the tools it may make public, and
/// whether to deploy over the breaks callers would see.
#[derive(Deserialize)]
struct Confirmed {
    #[serde(default)]
    public: String,
    #[serde(default)]
    force: bool,
}

/// Deploy the image in the body under `name`. `PUT` because the name is the
/// address and redeploying the same name replaces what is there.
async fn deploy(
    State(service): State<Arc<Service>>,
    Path(name): Path<String>,
    Query(confirmed): Query<Confirmed>,
    image: Bytes,
) -> Result<Json<Wire>, Denied> {
    let public: Vec<String> = confirmed
        .public
        .split(',')
        .filter(|tool| !tool.is_empty())
        .map(str::to_owned)
        .collect();
    let (deployed, breaks) = service
        .deploy(&name, image.to_vec(), &public, confirmed.force)
        .await?;
    let mut wire = service.describe(&deployed);
    wire.breaks = breaks.iter().map(ToString::to_string).collect();
    Ok(Json(wire))
}

/// The caller a tool call is made as, from [`host::CALLER`], unverified.
struct Called(Option<Caller>);

impl<S: Send + Sync> FromRequestParts<S> for Called {
    type Rejection = Denied;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Denied> {
        let Some(value) = parts.headers.get(host::CALLER) else {
            return Ok(Self(None));
        };
        let caller = serde_json::from_slice(value.as_bytes()).map_err(|e| {
            deny(
                StatusCode::BAD_REQUEST,
                format!("{} is not a caller: {e}", host::CALLER),
            )
        })?;
        Ok(Self(Some(caller)))
    }
}

/// Run one tool. `POST` because an invocation is not idempotent, and the body
/// is the argument object byte for byte as the program will read it.
async fn call(
    State(service): State<Arc<Service>>,
    Called(caller): Called,
    Path((name, tool)): Path<(String, String)>,
    arguments: Bytes,
) -> Result<Response, Denied> {
    run(&service, &name, caller, &tool, arguments.to_vec(), false).await
}

/// Run one tool dry: its arguments and how its fetches are answered in the
/// body, nothing it changes kept, answered as a call is.
async fn dry(
    State(service): State<Arc<Service>>,
    Called(caller): Called,
    Path((name, tool)): Path<(String, String)>,
    Json(run): Json<DryRun>,
) -> Result<Response, Denied> {
    let (headers, output) = service
        .host
        .dry_run(&name, caller.as_ref(), &tool, run.arguments(), run.dry)
        .await?;
    Ok((headers, Json(output)).into_response())
}

/// `?args=` on a `GET`: the argument object, URL-encoded JSON.
#[derive(Deserialize)]
struct Arguments {
    #[serde(default)]
    args: String,
}

/// Run one `query` tool, its argument object in `?args=`.
async fn query(
    State(service): State<Arc<Service>>,
    Called(caller): Called,
    Path((name, tool)): Path<(String, String)>,
    Query(arguments): Query<Arguments>,
) -> Result<Response, Denied> {
    run(
        &service,
        &name,
        caller,
        &tool,
        arguments.args.into_bytes(),
        true,
    )
    .await
}

/// Run `tool` on `name` as `caller`, answering with the headers the host
/// gives the answer, its meter among them. `query` holds it to a tool its
/// manifest marks `query`.
async fn run(
    service: &Service,
    name: &str,
    caller: Option<Caller>,
    tool: &str,
    arguments: Vec<u8>,
    query: bool,
) -> Result<Response, Denied> {
    let (headers, output) = service
        .host
        .call(name, caller.as_ref(), tool, arguments, query)
        .await?;
    Ok((headers, Json(output)).into_response())
}

/// A program's meters over the last 30 days, a day each.
async fn meters(
    State(service): State<Arc<Service>>,
    Path(name): Path<String>,
) -> Result<Json<Vec<Metered>>, Denied> {
    Ok(Json(service.host.meters(&name).await?))
}

/// A program's logs, oldest first, after the sequence number `after`.
async fn logs(
    State(service): State<Arc<Service>>,
    Path(name): Path<String>,
    Query(page): Query<Page>,
) -> Result<Json<Vec<Logged>>, Denied> {
    Ok(Json(
        service
            .host
            .logs(&name, page.topic, page.after, page.limit)
            .await?,
    ))
}

async fn undeploy(
    State(service): State<Arc<Service>>,
    Path(name): Path<String>,
) -> Result<StatusCode, Denied> {
    match service.undeploy(&name).await? {
        true => Ok(StatusCode::NO_CONTENT),
        false => Err(missing(&name)),
    }
}

fn missing(name: &str) -> Denied {
    deny(
        StatusCode::NOT_FOUND,
        format!("no program named {name:?} is deployed"),
    )
}
