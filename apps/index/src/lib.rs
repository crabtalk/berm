//! A local stand-in for an index service.
//!
//! Publishing to a real index means holding a credential for a git repository,
//! which is a deployment's business and not a laptop's. This serves the same
//! two routes over a plain directory so `berm publish` and `berm search` can be
//! exercised end to end without one.
//!
//! It is deliberately not the real thing: no git, no credential, no identity,
//! and it re-reads the directory on every request rather than holding it.

use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::{Path as Route, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use berm_api::Failed;
use berm_index::{Entry, Index};
use berm_oci::{Access, Reference, Registry};
use serde::Deserialize;
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

pub async fn serve(root: PathBuf, addr: SocketAddr) -> Result<()> {
    tokio::fs::create_dir_all(&root)
        .await
        .with_context(|| format!("cannot open {}", root.display()))?;

    let app = Router::new()
        .route("/berm", get(search).post(publish))
        .route("/berm/{*key}", get(versions))
        .with_state(Arc::new(root));

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;
    tracing::info!("listening on http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .context("server failed")
}

#[derive(Deserialize)]
struct Search {
    #[serde(default)]
    q: String,
}

async fn search(
    State(root): State<Arc<PathBuf>>,
    Query(search): Query<Search>,
) -> Result<Json<Vec<Entry>>, Refused> {
    let index = load(&root)?;
    Ok(Json(index.search(&search.q).into_iter().cloned().collect()))
}

async fn versions(
    State(root): State<Arc<PathBuf>>,
    Route(key): Route<String>,
) -> Result<Json<Vec<Entry>>, Refused> {
    match load(&root)?.versions(&key) {
        Some(entries) => Ok(Json(entries.to_vec())),
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

async fn publish(
    State(root): State<Arc<PathBuf>>,
    Json(request): Json<Publish>,
) -> Result<Json<Entry>, Refused> {
    let entry = record(&root, &request.reference)
        .await
        // The reference is the caller's: an unreadable artifact is their
        // business, not an outage here.
        .map_err(|error| Refused(StatusCode::BAD_REQUEST, format!("{error:#}")))?;
    Ok(Json(entry))
}

/// Pull what the reference says it is, and append a line for it.
async fn record(root: &Path, reference: &str) -> Result<Entry> {
    let reference = Reference::from_str(reference)?;
    let name = reference.to_string();

    // Anonymously, as a real index would: a program nobody can pull is not one
    // to list.
    let (digest, manifest) = tokio::task::spawn_blocking(move || {
        Registry::open(&reference, Access::Read)?.describe(&reference.reference)
    })
    .await
    .context("the registry lookup panicked")??;

    let entry = Entry::new(name, digest, None, manifest);
    let path = root.join(format!("{}.json", entry.key()));
    let parent = path.parent().context("an index path has no parent")?;
    tokio::fs::create_dir_all(parent).await?;

    let mut body = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    body.push_str(&serde_json::to_string(&entry)?);
    body.push('\n');
    tokio::fs::write(&path, body).await?;
    Ok(entry)
}

/// Read the directory per request. A stand-in has no reason to cache, and a
/// stale answer while testing is worse than a slow one.
fn load(root: &Path) -> Result<Index, Refused> {
    Index::load(root).map_err(|error| {
        Refused(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("cannot read the index: {error:#}"),
        )
    })
}

struct Refused(StatusCode, String);

impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        (self.0, Json(Failed { error: self.1 })).into_response()
    }
}
