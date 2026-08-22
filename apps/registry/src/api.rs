//! The index API.
//!
//! Resource-shaped, like bermd's, for the same reason: the clients are a CLI, a
//! browser and `curl`, and a harness is a resource.

use crate::{Entry, Index};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    response::{IntoResponse, Response},
    routing::get,
};
use berm_api::Failed;
use serde::Deserialize;
use std::sync::Arc;

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
    headers: HeaderMap,
    Json(request): Json<Publish>,
) -> Result<Json<Entry>, Refused> {
    let token = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| {
            Refused(
                StatusCode::UNAUTHORIZED,
                "publishing needs a GitHub token in Authorization".to_owned(),
            )
        })?;

    let entry = index
        .publish(&request.reference, token)
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
