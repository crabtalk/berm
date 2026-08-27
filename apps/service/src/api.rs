//! The control API: what a UI or a CLI drives the service with.
//!
//! Resource-shaped, on one endpoint, for the reason dockerd is: the clients are
//! a UI, a CLI and `curl`, and a harness is a resource. The MCP endpoint that
//! agents reach sits on the same server at `/mcp`.

use crate::Service;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use berm::Harness;
use berm_api::{Failed, Harness as Wire, Output};
use rmcp::transport::{
    StreamableHttpService, streamable_http_server::session::local::LocalSessionManager,
};
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
        .route("/harnesses", get(list))
        .route(
            "/harnesses/{name}",
            get(inspect).put(deploy).delete(undeploy),
        )
        .route("/harnesses/{name}/tools/{tool}", post(call))
        .route_service("/mcp", mcp)
        .with_state(service)
}

/// What berm read out of the ELF, on the wire.
fn describe(deployed: &Arc<Harness>) -> Wire {
    let manifest = deployed.manifest();
    Wire {
        name: deployed.name.to_string(),
        digest: deployed.digest.clone(),
        usage: manifest.usage.clone(),
        tools: manifest.tools.clone(),
    }
}

async fn list(State(service): State<Arc<Service>>) -> Json<Vec<Wire>> {
    Json(service.list().iter().map(describe).collect())
}

async fn inspect(
    State(service): State<Arc<Service>>,
    Path(name): Path<String>,
) -> Result<Json<Wire>, Refused> {
    match service.get(&name) {
        Some(deployed) => Ok(Json(describe(&deployed))),
        None => Err(Refused::missing(&name)),
    }
}

/// Deploy the ELF in the body under `name`. `PUT` because the name is the
/// address and redeploying the same name replaces what is there.
async fn deploy(
    State(service): State<Arc<Service>>,
    Path(name): Path<String>,
    elf: Bytes,
) -> Result<Json<Wire>, Refused> {
    let deployed = service
        .deploy(&name, elf.to_vec())
        .await
        // A rejected image is the client's ELF, not the service's fault: a bad
        // name, an unreadable ELF, a manifest that disagrees with its exports.
        .map_err(|error| Refused(StatusCode::BAD_REQUEST, format!("{error:#}")))?;
    Ok(Json(describe(&deployed)))
}

/// Run one tool. `POST` because an invocation is not idempotent, and the body
/// is the argument object byte for byte as the harness will read it.
async fn call(
    State(service): State<Arc<Service>>,
    Path((name, tool)): Path<(String, String)>,
    arguments: Bytes,
) -> Result<Json<Output>, Refused> {
    let Some(deployed) = service.get(&name) else {
        return Err(Refused::missing(&name));
    };
    // Asked here rather than left to the invocation, which reports a missing
    // tool through the same error as a trap — and the two are not the same
    // status.
    if !deployed
        .manifest()
        .tools
        .iter()
        .any(|spec| spec.name == tool)
    {
        return Err(Refused(
            StatusCode::NOT_FOUND,
            format!("harness {name:?} exports no tool named {tool:?}"),
        ));
    }

    match service.call(&name, &tool, arguments.to_vec()).await {
        Ok(outcome) => Ok(Json(outcome.into())),
        // The harness is there and so is the tool, so what is left is the
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
            format!("no harness named {name:?} is deployed"),
        )
    }
}

impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        (self.0, Json(Failed { error: self.1 })).into_response()
    }
}
