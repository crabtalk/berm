//! Talking to the host a context names: a bermd, or Cloud.

use crate::{
    context::{Context as Target, Owner},
    http,
};
use anyhow::{Context, Result, bail};
use berm_api::{
    Budget, DryRun, Logged, Meter, Output, Price, Program, Terms,
    host::{self, Caller, Metered},
};
use reqwest::blocking::RequestBuilder;

pub struct Client {
    pub host: String,
    /// The bearer a Cloud context carries.
    pub token: Option<String>,
    /// The account a Cloud context deploys under; `None` for a bermd.
    pub owner: Option<Owner>,
    http: reqwest::blocking::Client,
}

impl Client {
    pub fn new(target: Target) -> Self {
        Self {
            host: target.host.trim_end_matches('/').to_owned(),
            token: target.token,
            owner: target.owner,
            http: reqwest::blocking::Client::new(),
        }
    }

    pub fn list(&self) -> Result<Vec<Program>> {
        let collection = match &self.owner {
            Some(owner) => format!("{}{}", self.host, owner.services()),
            None => format!("{}/programs", self.host),
        };
        self.read(self.http.get(collection))?
            .json()
            .context("the host returned something that is not a program list")
    }

    pub fn inspect(&self, name: &str) -> Result<Program> {
        self.read(self.http.get(self.url(name)))?
            .json()
            .context("the host returned something that is not a program")
    }

    /// Deploy `image` as `name` on a bermd, confirming the tools it may make
    /// public.
    pub fn deploy(
        &self,
        name: &str,
        image: Vec<u8>,
        public: &[String],
        force: bool,
    ) -> Result<Program> {
        if self.owner.is_some() {
            bail!("Cloud deploys from the program index, not from bytes");
        }
        let request = self
            .http
            .put(self.url(name))
            .query(&[("public", public.join(","))])
            .query(&[("force", force)])
            .body(image);
        self.read(request)?
            .json()
            .context("bermd returned something that is not a program")
    }

    /// Deploy the index entry `reference` as `name` on Cloud, confirming the
    /// tools it may make public.
    pub fn deploy_reference(
        &self,
        name: &str,
        reference: &str,
        public: &[String],
        force: bool,
    ) -> Result<Program> {
        let request = self.http.put(self.url(name)).json(&serde_json::json!({
            "reference": reference,
            "public": public,
            "force": force,
        }));
        self.read(request)?
            .json()
            .context("Cloud returned something that is not a program")
    }

    /// Run `tool` of `program`, as `caller` on a bermd, which takes it
    /// unverified.
    pub fn run(
        &self,
        program: &str,
        tool: &str,
        arguments: Vec<u8>,
        caller: Option<&Caller>,
    ) -> Result<(Output, Option<Meter>)> {
        let url = format!("{}/tools/{tool}", self.url(program));
        let mut request = self.http.post(url).body(arguments);
        if let Some(caller) = caller {
            if self.owner.is_some() {
                bail!("Cloud calls as the context's token; `--caller` is for bermd");
            }
            request = request.header(host::CALLER, serde_json::to_string(caller)?);
        }
        answered(self.read(request)?)
    }

    /// Run `tool` of `program` dry, as `run` says.
    pub fn dry(
        &self,
        program: &str,
        tool: &str,
        run: &DryRun,
        caller: Option<&Caller>,
    ) -> Result<(Output, Option<Meter>)> {
        let url = format!("{}/tools/{tool}/dry", self.url(program));
        let mut request = self.http.post(url).json(run);
        if let Some(caller) = caller {
            if self.owner.is_some() {
                bail!("Cloud calls as the context's token; `--caller` is for bermd");
            }
            request = request.header(host::CALLER, serde_json::to_string(caller)?);
        }
        answered(self.read(request)?)
    }

    /// What a call of each of `name`'s public tools is held to.
    pub fn terms(&self, name: &str) -> Result<Terms> {
        self.read(self.http.get(format!("{}/terms", self.url(name))))?
            .json()
            .context("the host returned something that is not terms")
    }

    /// Set what one call of `tool` of `name` may use.
    pub fn budget(&self, name: &str, tool: &str, budget: Budget) -> Result<()> {
        let url = format!("{}/tools/{tool}/budget", self.url(name));
        self.read(self.http.put(url).json(&budget))?;
        Ok(())
    }

    /// Set what a call of `tool` of `name` that ends `done` costs.
    pub fn price(&self, name: &str, tool: &str, price: Price) -> Result<()> {
        let url = format!("{}/tools/{tool}/price", self.url(name));
        self.read(self.http.put(url).json(&price))?;
        Ok(())
    }

    /// `name`'s meters over the last 30 days, a day each.
    pub fn meters(&self, name: &str) -> Result<Vec<Metered>> {
        self.read(self.http.get(format!("{}/meters", self.url(name))))?
            .json()
            .context("the host returned something that is not a list of meters")
    }

    pub fn undeploy(&self, name: &str) -> Result<()> {
        self.read(self.http.delete(self.url(name)))?;
        Ok(())
    }

    /// `name`'s logs after `after`, oldest first, at most `limit`.
    pub fn logs(&self, name: &str, after: u64, limit: u64) -> Result<Vec<Logged>> {
        let request = self
            .http
            .get(format!("{}/logs", self.url(name)))
            .query(&[("after", after), ("limit", limit)]);
        self.read(request)?
            .json()
            .context("the host returned something that is not a list of logs")
    }

    fn url(&self, name: &str) -> String {
        match &self.owner {
            Some(owner) => format!("{}/services/{}/{name}", self.host, owner.name()),
            None => format!("{}/programs/{name}", self.host),
        }
    }

    fn read(&self, request: RequestBuilder) -> Result<reqwest::blocking::Response> {
        let request = match &self.token {
            Some(token) => request.bearer_auth(token),
            None => request,
        };
        http::read(&self.host, request.send())
    }
}

/// A tool's answer and the meter its headers carry.
fn answered(response: reqwest::blocking::Response) -> Result<(Output, Option<Meter>)> {
    let meter = response
        .headers()
        .get(host::METER)
        .and_then(|value| serde_json::from_slice(value.as_bytes()).ok());
    let output = response
        .json()
        .context("the host returned something that is not a tool result")?;
    Ok((output, meter))
}
