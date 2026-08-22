//! Talking to bermd.

use crate::http;
use anyhow::{Context, Result};
use berm_api::{Harness, Output};

pub struct Client {
    host: String,
    http: reqwest::blocking::Client,
}

impl Client {
    pub fn new(host: String) -> Self {
        Self {
            host: host.trim_end_matches('/').to_owned(),
            http: reqwest::blocking::Client::new(),
        }
    }

    pub fn list(&self) -> Result<Vec<Harness>> {
        let collection = format!("{}/harnesses", self.host);
        http::read(&self.host, self.http.get(collection).send())?
            .json()
            .context("bermd returned something that is not a harness list")
    }

    pub fn inspect(&self, name: &str) -> Result<Harness> {
        http::read(&self.host, self.http.get(self.url(name)).send())?
            .json()
            .context("bermd returned something that is not a harness")
    }

    pub fn deploy(&self, name: &str, elf: Vec<u8>) -> Result<Harness> {
        http::read(&self.host, self.http.put(self.url(name)).body(elf).send())?
            .json()
            .context("bermd returned something that is not a harness")
    }

    pub fn run(&self, harness: &str, tool: &str, arguments: Vec<u8>) -> Result<Output> {
        let url = format!("{}/tools/{tool}", self.url(harness));
        http::read(&self.host, self.http.post(url).body(arguments).send())?
            .json()
            .context("bermd returned something that is not a tool result")
    }

    pub fn undeploy(&self, name: &str) -> Result<()> {
        http::read(&self.host, self.http.delete(self.url(name)).send())?;
        Ok(())
    }

    fn url(&self, name: &str) -> String {
        format!("{}/harnesses/{name}", self.host)
    }
}
