//! The host every berm daemon runs: the services it holds, the state they
//! commit, and the calls into them. A daemon builds a [`Host`] over its own
//! [`Env`], authenticates its callers and serves its own routes; everything a
//! program observes is the host's.

use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::IntoResponse,
};
use berm_api::Failed;

pub use config::Config;
pub use env::{Env, Images, Peers, Secrets};
pub use host::Host;

mod call;
mod callers;
mod commit;
mod config;
mod deliver;
mod env;
mod fetch;
mod host;
mod limit;
mod routes;
mod service;
mod store;
mod usage;
mod wake;

/// A refusal the caller can read, as `{"error": message}`.
pub fn deny(status: StatusCode, message: impl Into<String>) -> Denied {
    Denied(status, message.into())
}

#[derive(Debug)]
pub struct Denied(pub StatusCode, pub String);

impl IntoResponse for Denied {
    fn into_response(self) -> axum::response::Response {
        let mut response = (self.0, Json(Failed { error: self.1 })).into_response();
        // Only a tool answers `405`, and every tool takes `POST`.
        if self.0 == StatusCode::METHOD_NOT_ALLOWED {
            response
                .headers_mut()
                .insert(header::ALLOW, HeaderValue::from_static("POST"));
        }
        response
    }
}

impl From<anyhow::Error> for Denied {
    fn from(e: anyhow::Error) -> Self {
        tracing::error!("host failed: {e:#}");
        deny(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
    }
}
