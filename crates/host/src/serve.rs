//! The routes a host serves for one service, for a daemon to nest under the
//! path it names services by.

use crate::{Denied, env::Env, host::Host};
use axum::{
    Extension, Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{AppendHeaders, IntoResponse, Response},
    routing::{get, post, put},
};
use berm_api::{
    Budget, Cron, DryRun, Listing, Program, Runtime, Terms,
    host::{self, CallPage, Caller, Crons, Live, Metered, Page, ToolUpdate, Usage, Wakes},
};
use serde::Deserialize;
use std::sync::Arc;

/// The service a request is for, as its daemon names it. The daemon puts it in
/// the request's extensions before [`router`] runs, and the [`Caller`] a call
/// is made as beside it when there is one.
#[derive(Debug, Clone)]
pub struct Named(pub String);

/// One service's routes, relative to where the daemon nests them.
pub fn router<E: Env>(host: Host<E>) -> Router {
    Router::new()
        .route("/", get(listing::<E>))
        .route("/idl", get(idl::<E>))
        .route("/runtime", get(runtime::<E>))
        .route("/terms", get(terms::<E>))
        .route("/usage", get(usage::<E>))
        .route("/meters", get(meters::<E>))
        .route("/live", get(live::<E>))
        .route("/logs", get(logs::<E>))
        .route("/calls", get(calls::<E>))
        .route("/crons", get(crons::<E>))
        .route("/wakes", get(wakes::<E>))
        .route(
            "/tools/{tool}",
            post(call::<E>).get(query::<E>).patch(update::<E>),
        )
        .route("/tools/{tool}/dry", post(dry::<E>))
        .route("/tools/{tool}/budget", put(budget::<E>))
        .route("/tools/{tool}/cron", put(cron::<E>))
        .with_state(host)
}

/// The service as anyone it is visible to sees it.
async fn listing<E: Env>(
    State(host): State<Host<E>>,
    Extension(Named(service)): Extension<Named>,
) -> Result<Json<Listing>, Denied> {
    Ok(Json(host.listing(&service).await?))
}

async fn idl<E: Env>(
    State(host): State<Host<E>>,
    Extension(Named(service)): Extension<Named>,
) -> Result<Json<Program>, Denied> {
    Ok(Json(host.idl(&service).await?))
}

/// How the service runs, for its owners.
async fn runtime<E: Env>(
    State(host): State<Host<E>>,
    Extension(Named(service)): Extension<Named>,
) -> Result<Json<Runtime>, Denied> {
    Ok(Json(host.runtime(&service).await?))
}

/// What a call of each public tool is held to.
async fn terms<E: Env>(
    State(host): State<Host<E>>,
    Extension(Named(service)): Extension<Named>,
) -> Result<Json<Terms>, Denied> {
    Ok(Json(host.terms(&service)?))
}

/// Who uses the service.
async fn usage<E: Env>(
    State(host): State<Host<E>>,
    Extension(Named(service)): Extension<Named>,
) -> Result<Json<Vec<Usage>>, Denied> {
    Ok(Json(host.usage(&service).await?))
}

/// What the service used over the last 30 days, a day each.
async fn meters<E: Env>(
    State(host): State<Host<E>>,
    Extension(Named(service)): Extension<Named>,
) -> Result<Json<Vec<Metered>>, Denied> {
    Ok(Json(host.meters(&service).await?))
}

/// What the service is using now.
async fn live<E: Env>(
    State(host): State<Host<E>>,
    Extension(Named(service)): Extension<Named>,
) -> Result<Json<Live>, Denied> {
    Ok(Json(host.live(&service).await?))
}

/// The service's logs, oldest first, after the sequence number `after`, a
/// page at a time, with the other pages in `Link`.
async fn logs<E: Env>(
    State(host): State<Host<E>>,
    Extension(Named(service)): Extension<Named>,
    Query(page): Query<Page>,
) -> Result<impl IntoResponse, Denied> {
    let (logs, link) = host.logs(&service, page).await?;
    Ok((
        AppendHeaders(link.map(|link| (header::LINK, link))),
        Json(logs),
    ))
}

/// The service's runs, newest first, a page at a time, with the other pages
/// in `Link` and how many runs match in [`host::TOTAL`].
async fn calls<E: Env>(
    State(host): State<Host<E>>,
    Extension(Named(service)): Extension<Named>,
    Query(page): Query<CallPage>,
) -> Result<impl IntoResponse, Denied> {
    let (calls, total, link) = host.calls(&service, page).await?;
    Ok((
        AppendHeaders(link.map(|link| (header::LINK, link))),
        [(host::TOTAL, total.to_string())],
        Json(calls),
    ))
}

/// The service's cron tools.
async fn crons<E: Env>(
    State(host): State<Host<E>>,
    Extension(Named(service)): Extension<Named>,
) -> Result<Json<Crons>, Denied> {
    Ok(Json(host.crons(&service).await?))
}

/// The wakes armed to call the service's tools that have not run.
async fn wakes<E: Env>(
    State(host): State<Host<E>>,
    Extension(Named(service)): Extension<Named>,
) -> Result<Json<Wakes>, Denied> {
    Ok(Json(host.armed(&service).await?))
}

/// Run one tool. `POST` because an invocation is not idempotent, and the body
/// is the argument object byte for byte as the program will read it.
async fn call<E: Env>(
    State(host): State<Host<E>>,
    Extension(Named(service)): Extension<Named>,
    caller: Option<Extension<Caller>>,
    Path(Tool { tool }): Path<Tool>,
    headers: HeaderMap,
    args: Bytes,
) -> Result<Response, Denied> {
    let caller = caller.map(|Extension(caller)| caller);
    let called = host
        .outside(&service, &tool, false, args.to_vec(), None, caller, headers)
        .await?;
    Ok((called.headers, Json(called.output)).into_response())
}

/// The tool a route names, among the daemon's own path parameters.
#[derive(Deserialize)]
struct Tool {
    tool: String,
}

/// `?args=` on a `GET`: the argument object, URL-encoded JSON.
#[derive(Deserialize)]
struct Arguments {
    #[serde(default)]
    args: String,
}

/// Run one `query` tool, its argument object in `?args=`. The `ETag` is the
/// service's commit count.
async fn query<E: Env>(
    State(host): State<Host<E>>,
    Extension(Named(service)): Extension<Named>,
    caller: Option<Extension<Caller>>,
    Path(Tool { tool }): Path<Tool>,
    Query(arguments): Query<Arguments>,
    headers: HeaderMap,
) -> Result<Response, Denied> {
    let commits = host.commits(&service).await?;
    let etag =
        HeaderValue::from_str(&format!("\"{commits}\"")).expect("a number is a header value");
    if headers.get(header::IF_NONE_MATCH) == Some(&etag) {
        return Ok((StatusCode::NOT_MODIFIED, [(header::ETAG, etag)]).into_response());
    }
    let caller = caller.map(|Extension(caller)| caller);
    let mut called = host
        .outside(
            &service,
            &tool,
            true,
            arguments.args.into_bytes(),
            None,
            caller,
            headers,
        )
        .await?;
    called.headers.insert(header::ETAG, etag);
    Ok((called.headers, Json(called.output)).into_response())
}

/// Run one tool dry: its arguments and how its fetches are answered in the
/// body, nothing it changes kept, answered as a call is.
async fn dry<E: Env>(
    State(host): State<Host<E>>,
    Extension(Named(service)): Extension<Named>,
    caller: Option<Extension<Caller>>,
    Path(Tool { tool }): Path<Tool>,
    headers: HeaderMap,
    Json(run): Json<DryRun>,
) -> Result<Response, Denied> {
    let caller = caller.map(|Extension(caller)| caller);
    let called = host
        .outside(
            &service,
            &tool,
            false,
            run.arguments(),
            Some(Arc::new(run.dry)),
            caller,
            headers,
        )
        .await?;
    Ok((called.headers, Json(called.output)).into_response())
}

/// Set or clear a deprecated tool's sunset.
async fn update<E: Env>(
    State(host): State<Host<E>>,
    Extension(Named(service)): Extension<Named>,
    Path(Tool { tool }): Path<Tool>,
    Json(update): Json<ToolUpdate>,
) -> Result<StatusCode, Denied> {
    host.sunset(&service, &tool, update.sunset).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Set what one call of a tool may use, from its next call.
async fn budget<E: Env>(
    State(host): State<Host<E>>,
    Extension(Named(service)): Extension<Named>,
    Path(Tool { tool }): Path<Tool>,
    Json(budget): Json<Budget>,
) -> Result<StatusCode, Denied> {
    host.set_budget(&service, &tool, budget).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Set a cron tool's expression, until a deploy's config sets another.
async fn cron<E: Env>(
    State(host): State<Host<E>>,
    Extension(Named(service)): Extension<Named>,
    Path(Tool { tool }): Path<Tool>,
    Json(Cron { cron }): Json<Cron>,
) -> Result<StatusCode, Denied> {
    host.set_cron(&service, &tool, cron).await?;
    Ok(StatusCode::NO_CONTENT)
}
