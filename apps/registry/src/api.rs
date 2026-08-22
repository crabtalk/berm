//! The index API.
//!
//! Resource-shaped, like bermd's, for the same reason: the clients are a CLI, a
//! browser and `curl`, and a harness is a resource.

use crate::{Caller, Entry, Index};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use berm_api::Failed;
use serde::Deserialize;
use std::sync::Arc;

/// The index's routes, carrying no authentication of their own.
///
/// A service that mounts these owns the policy over them — a scope, a session,
/// a throttle — and says who is publishing by inserting a [`Caller`]. Mounted
/// bare, as the standalone binary does, the index is open.
pub fn router(index: Arc<Index>) -> Router {
    Router::new()
        .route("/harnesses", get(search).post(publish))
        // A wildcard, because the key is a registry and a repository path —
        // `ghcr.io/clearloop/fs` — and the slashes in it are not route
        // separators.
        .route("/harnesses/{*key}", get(versions))
        .with_state(index)
}

#[derive(Deserialize)]
struct Search {
    #[serde(default)]
    q: String,
}

async fn search(State(index): State<Arc<Index>>, Query(search): Query<Search>) -> Json<Vec<Entry>> {
    Json(index.search(&search.q).await)
}

async fn versions(
    State(index): State<Arc<Index>>,
    Path(key): Path<String>,
) -> Result<Json<Vec<Entry>>, Refused> {
    match index.versions(&key).await {
        Some(entries) => Ok(Json(entries)),
        None => Err(Refused(
            StatusCode::NOT_FOUND,
            format!("nothing published under {key:?}"),
        )),
    }
}

#[derive(Deserialize)]
struct Publish {
    reference: String,
}

/// `POST` rather than `PUT`: the index appends a version, and the caller does
/// not choose where it lands.
async fn publish(
    State(index): State<Arc<Index>>,
    caller: Option<Extension<Caller>>,
    Json(request): Json<Publish>,
) -> Result<Json<Entry>, Refused> {
    let entry = index
        .publish(&request.reference, caller.map(|Extension(who)| who.0))
        .await
        // The reference and the token are both the caller's: an unreadable
        // artifact or a token GitHub will not vouch for is their business, not
        // an outage here.
        .map_err(|error| Refused(StatusCode::BAD_REQUEST, format!("{error:#}")))?;
    Ok(Json(entry))
}

struct Refused(StatusCode, String);

impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        (self.0, Json(Failed { error: self.1 })).into_response()
    }
}
