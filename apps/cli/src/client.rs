//! Talking to bermd.

use anyhow::{Context, Result, bail};
use berm_api::{Failed, Harness};
use reqwest::blocking::Response;

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
        self.read(self.http.get(collection).send())?
            .json()
            .context("bermd returned something that is not a harness list")
    }

    pub fn inspect(&self, name: &str) -> Result<Harness> {
        self.read(self.http.get(self.url(name)).send())?
            .json()
            .context("bermd returned something that is not a harness")
    }

    pub fn deploy(&self, name: &str, elf: Vec<u8>) -> Result<Harness> {
        self.read(self.http.put(self.url(name)).body(elf).send())?
            .json()
            .context("bermd returned something that is not a harness")
    }

    pub fn undeploy(&self, name: &str) -> Result<()> {
        self.read(self.http.delete(self.url(name)).send())?;
        Ok(())
    }

    fn url(&self, name: &str) -> String {
        format!("{}/harnesses/{name}", self.host)
    }

    /// Turn a refusal into the message bermd sent, rather than a status code
    /// the operator then has to go and look up.
    fn read(&self, response: reqwest::Result<Response>) -> Result<Response> {
        let response = response.with_context(|| format!("cannot reach bermd at {}", self.host))?;
        if response.status().is_success() {
            return Ok(response);
        }

        let status = response.status();
        match response.json::<Failed>() {
            Ok(failed) => bail!("{}", failed.error),
            Err(_) => bail!("bermd answered {status}"),
        }
    }
}
