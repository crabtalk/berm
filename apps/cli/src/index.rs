//! Talking to a harness index.
//!
//! Which index is the caller's choice: there is no default here, because a
//! built-in one would make berm ship an opinion about whose list you read.

use crate::http;
use anyhow::{Context, Result};
use berm_api::ToolSpec;
use serde::Deserialize;

/// Where to look, when `--index` is not given.
const INDEX: &str = "BERM_INDEX";

/// One published version, as the index reports it.
#[derive(Deserialize)]
pub struct Entry {
    pub reference: String,
    pub digest: String,
    pub publisher: String,
    pub usage: String,
    pub tools: Vec<ToolSpec>,
}

pub struct Index {
    host: String,
    http: reqwest::blocking::Client,
}

impl Index {
    pub fn new(index: Option<&String>) -> Result<Self> {
        let host = match index {
            Some(index) => index.clone(),
            None => {
                std::env::var(INDEX).context("no index given: pass --index, or set BERM_INDEX")?
            }
        };
        Ok(Self {
            host: host.trim_end_matches('/').to_owned(),
            http: reqwest::blocking::Client::new(),
        })
    }

    pub fn search(&self, term: &str) -> Result<Vec<Entry>> {
        let url = format!("{}/harnesses?q={term}", self.host);
        http::read(&self.host, self.http.get(url).send())?
            .json()
            .context("the index returned something that is not a harness list")
    }

    pub fn publish(&self, reference: &str, token: &str) -> Result<Entry> {
        let url = format!("{}/harnesses", self.host);
        let request = self
            .http
            .post(url)
            .bearer_auth(token)
            .json(&serde_json::json!({ "reference": reference }));
        http::read(&self.host, request.send())?
            .json()
            .context("the index returned something that is not a harness")
    }
}
