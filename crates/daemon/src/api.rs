//! The control API: what a UI or a CLI drives the service with.
//!
//! Resource-shaped, on one endpoint, for the reason dockerd is: the clients are
//! a UI, a CLI and `curl`, and a program is a resource. The MCP endpoint that
//! agents reach sits on the same server at `/mcp`.

use crate::Service;
use axum::{
    Extension, Json, Router,
    body::Bytes,
    extract::{Path, Query, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    routing::{get, put},
};
use berm_api::{Deployed, Program as Wire, host};
use berm_host::{Denied, Named, deny};
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
    let program = berm_host::router(service.host.clone())
        .merge(
            Router::new()
                .route("/", put(deploy).delete(undeploy))
                .with_state(service.clone()),
        )
        .route_layer(middleware::from_fn(named));

    Router::new()
        .route("/programs", get(list))
        .with_state(service)
        .nest("/programs/{name}", program)
        .route_service("/mcp", mcp)
}

/// The program a request names.
#[derive(Deserialize)]
struct Name {
    name: String,
}

/// Name the program a request is for, and the caller it is made as, from
/// [`host::CALLER`], unverified.
async fn named(
    Path(Name { name }): Path<Name>,
    mut request: Request,
    next: Next,
) -> Result<Response, Denied> {
    if let Some(value) = request.headers().get(host::CALLER) {
        let caller: host::Caller = serde_json::from_slice(value.as_bytes()).map_err(|e| {
            deny(
                StatusCode::BAD_REQUEST,
                format!("{} is not a caller: {e}", host::CALLER),
            )
        })?;
        request.extensions_mut().insert(caller);
    }
    request.extensions_mut().insert(Named(name));
    Ok(next.run(request).await)
}

async fn list(State(service): State<Arc<Service>>) -> Json<Vec<Wire>> {
    Json(
        service
            .list()
            .iter()
            .map(|one| one.idl(&one.name))
            .collect(),
    )
}

/// `?public=a,b&force=true&crons={"tick":"*/5 * * * *"}` on a deploy: the
/// tools it may make public, whether to deploy over what it would break or
/// undo, and each cron tool's expression, as a JSON object, when the deploy
/// sets them.
#[derive(Deserialize)]
struct Confirmed {
    #[serde(default)]
    public: String,
    #[serde(default)]
    force: bool,
    crons: Option<String>,
}

/// Deploy the image in the body under `name`. `PUT` because the name is the
/// address and redeploying the same name replaces what is there.
async fn deploy(
    State(service): State<Arc<Service>>,
    Extension(Named(name)): Extension<Named>,
    Query(confirmed): Query<Confirmed>,
    image: Bytes,
) -> Result<Json<Deployed>, Denied> {
    let public: Vec<String> = confirmed
        .public
        .split(',')
        .filter(|tool| !tool.is_empty())
        .map(str::to_owned)
        .collect();
    let crons = confirmed
        .crons
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| {
            deny(
                StatusCode::BAD_REQUEST,
                format!("`crons` is not a JSON object of tool to cron expression: {e}"),
            )
        })?;
    let (deployed, breaks) = service
        .deploy(&name, image.to_vec(), &public, confirmed.force, crons)
        .await?;
    Ok(Json(Deployed {
        program: deployed.idl(&name),
        breaks: breaks.iter().map(ToString::to_string).collect(),
    }))
}

async fn undeploy(
    State(service): State<Arc<Service>>,
    Extension(Named(name)): Extension<Named>,
) -> Result<StatusCode, Denied> {
    match service.undeploy(&name).await? {
        true => Ok(StatusCode::NO_CONTENT),
        false => Err(deny(
            StatusCode::NOT_FOUND,
            format!("no program named {name:?} is deployed"),
        )),
    }
}
