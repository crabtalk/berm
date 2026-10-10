//! The control API: what a UI or a CLI drives the service with.
//!
//! Resource-shaped, on one endpoint, for the reason dockerd is: the clients are
//! a UI, a CLI and `curl`, and a program is a resource. The MCP endpoint that
//! agents reach sits on the same server at `/mcp`.

use crate::Service;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use berm::Program;
use berm_api::{Failed, Output, Program as Wire};
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
        .route_service("/mcp", mcp)
        .with_state(service)
}

impl Service {
    /// What berm read out of the image, on the wire, and what of it this service
    /// has nothing to answer with.
    fn describe(&self, deployed: &Arc<Program>) -> Wire {
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
) -> Result<Json<Wire>, Refused> {
    match service.get(&name) {
        Some(deployed) => Ok(Json(service.describe(&deployed))),
        None => Err(Refused::missing(&name)),
    }
}

/// `?public=a,b` on a deploy: the tools it may make public.
#[derive(Deserialize)]
struct Confirmed {
    #[serde(default)]
    public: String,
}

/// Deploy the image in the body under `name`. `PUT` because the name is the
/// address and redeploying the same name replaces what is there.
async fn deploy(
    State(service): State<Arc<Service>>,
    Path(name): Path<String>,
    Query(confirmed): Query<Confirmed>,
    image: Bytes,
) -> Result<Json<Wire>, Refused> {
    let public: Vec<String> = confirmed
        .public
        .split(',')
        .filter(|tool| !tool.is_empty())
        .map(str::to_owned)
        .collect();
    let (deployed, breaks) = service
        .deploy(&name, image.to_vec(), &public)
        .await
        // A rejected image is the client's, not the service's fault: a bad
        // name, an unreadable image, a manifest that disagrees with its
        // exports.
        .map_err(|error| Refused(StatusCode::BAD_REQUEST, format!("{error:#}")))?;
    let mut wire = service.describe(&deployed);
    wire.breaks = breaks.iter().map(ToString::to_string).collect();
    Ok(Json(wire))
}

/// Run one tool. `POST` because an invocation is not idempotent, and the body
/// is the argument object byte for byte as the program will read it.
async fn call(
    State(service): State<Arc<Service>>,
    Path((name, tool)): Path<(String, String)>,
    arguments: Bytes,
) -> Result<Json<Output>, Refused> {
    run(&service, &name, &tool, arguments.to_vec(), false).await
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
    Path((name, tool)): Path<(String, String)>,
    Query(arguments): Query<Arguments>,
) -> Result<Json<Output>, Refused> {
    run(&service, &name, &tool, arguments.args.into_bytes(), true).await
}

/// Run `tool` on `name`. `query` holds it to a tool its manifest marks `query`.
async fn run(
    service: &Service,
    name: &str,
    tool: &str,
    arguments: Vec<u8>,
    query: bool,
) -> Result<Json<Output>, Refused> {
    let Some(deployed) = service.get(name) else {
        return Err(Refused::missing(name));
    };
    // Asked here rather than left to the invocation, which reports a missing
    // tool through the same error as a trap — and the two are not the same
    // status.
    let Some(spec) = deployed
        .manifest()
        .tools
        .iter()
        .find(|spec| spec.name == tool && spec.public)
    else {
        return Err(Refused(
            StatusCode::NOT_FOUND,
            format!("program {name:?} has no public tool named {tool:?}"),
        ));
    };
    if query && !spec.query {
        return Err(Refused(
            StatusCode::METHOD_NOT_ALLOWED,
            format!("{name}.{tool} is not a query; POST it"),
        ));
    }

    match service.call(name, tool, arguments).await {
        Ok(outcome) => Ok(Json(outcome.into())),
        // The program is there and so is the tool, so what is left is the
        // invocation: a trap, or a guest that came back unreadable.
        Err(error) => Err(Refused(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("{error:#}"),
        )),
    }
}

async fn undeploy(
    State(service): State<Arc<Service>>,
    Path(name): Path<String>,
) -> Result<StatusCode, Refused> {
    match service.undeploy(&name).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(Refused::missing(&name)),
        Err(error) => Err(Refused(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("{error:#}"),
        )),
    }
}

struct Refused(StatusCode, String);

impl Refused {
    fn missing(name: &str) -> Self {
        Self(
            StatusCode::NOT_FOUND,
            format!("no program named {name:?} is deployed"),
        )
    }
}

impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        let mut response = (self.0, Json(Failed { error: self.1 })).into_response();
        // Only a tool answers `405`, and every tool takes `POST`.
        if self.0 == StatusCode::METHOD_NOT_ALLOWED {
            response
                .headers_mut()
                .insert(header::ALLOW, header::HeaderValue::from_static("POST"));
        }
        response
    }
}
