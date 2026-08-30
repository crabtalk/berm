//! Talking to bermd.

use crate::http;
use anyhow::{Context, Result};
use berm_api::{Output, Program};

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

    pub fn list(&self) -> Result<Vec<Program>> {
        let collection = format!("{}/programs", self.host);
        http::read(&self.host, self.http.get(collection).send())?
            .json()
            .context("bermd returned something that is not a program list")
    }

    pub fn inspect(&self, name: &str) -> Result<Program> {
        http::read(&self.host, self.http.get(self.url(name)).send())?
            .json()
            .context("bermd returned something that is not a program")
    }

    pub fn deploy(&self, name: &str, image: Vec<u8>) -> Result<Program> {
        http::read(&self.host, self.http.put(self.url(name)).body(image).send())?
            .json()
            .context("bermd returned something that is not a program")
    }

    pub fn run(&self, program: &str, tool: &str, arguments: Vec<u8>) -> Result<Output> {
        let url = format!("{}/tools/{tool}", self.url(program));
        http::read(&self.host, self.http.post(url).body(arguments).send())?
            .json()
            .context("bermd returned something that is not a tool result")
    }

    pub fn undeploy(&self, name: &str) -> Result<()> {
        http::read(&self.host, self.http.delete(self.url(name)).send())?;
        Ok(())
    }

    fn url(&self, name: &str) -> String {
        format!("{}/programs/{name}", self.host)
    }
}
