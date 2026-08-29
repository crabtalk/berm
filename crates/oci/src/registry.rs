//! Talking to an OCI registry.

use crate::Reference;
use anyhow::{Context, Result, bail};
use berm_api::Manifest;
use reqwest::{
    Method, StatusCode,
    blocking::{Client, RequestBuilder, Response},
    header::{ACCEPT, CONTENT_TYPE, LOCATION, WWW_AUTHENTICATE},
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::env;

/// The layer, and what the whole artifact is typed as.
pub const PROGRAM: &str = "application/vnd.berm.program.v1";
/// The config blob: what the program says it is.
pub const MANIFEST: &str = "application/vnd.berm.manifest.v1+json";
const OCI_MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";
/// Accepted only to be refused by name: a registry handed one of these back as
/// its own error otherwise, which read as a berm bug.
const OCI_INDEX: &str = "application/vnd.oci.image.index.v1+json";

/// Where a credential comes from. CI has this already; a laptop can export
/// `gh auth token`. Reading `~/.docker/config.json` is not done here.
const TOKEN: &str = "GITHUB_TOKEN";

/// What a token is asked for. A read token is issued anonymously for a public
/// repository; a write token never is.
#[derive(Clone, Copy)]
pub enum Access {
    Read,
    Write,
}

/// A registry, already holding a token for one repository.
pub struct Registry {
    base: String,
    repository: String,
    http: Client,
    token: Option<String>,
}

impl Registry {
    /// Take the token up front, so a blob upload is never sent twice to answer
    /// a challenge it could have answered before the body was built.
    pub fn open(reference: &Reference, access: Access) -> Result<Self> {
        let scheme = if loopback(&reference.registry) {
            "http"
        } else {
            "https"
        };
        let mut registry = Self {
            base: format!("{scheme}://{}", reference.registry),
            repository: reference.repository.clone(),
            http: Client::new(),
            token: None,
        };

        let action = match access {
            Access::Read => "pull",
            Access::Write => "pull,push",
        };
        let scope = format!("repository:{}:{action}", registry.repository);
        registry.authenticate(&scope)?;
        Ok(registry)
    }

    /// The image, verified against the digest the registry advertised for it.
    pub fn pull(&self, reference: &str) -> Result<Vec<u8>> {
        let artifact = self.artifact(reference)?;
        let layer = layer(&artifact)?;
        let digest = layer
            .get("digest")
            .and_then(Value::as_str)
            .context("the artifact's layer has no digest")?;
        self.blob(digest)
    }

    /// The image's digest and what it says it is, without downloading it.
    ///
    /// The config blob is the `.berm.abi` section, so this costs one small GET
    /// and never touches the image — which is the whole reason it is carried
    /// there rather than left in the ELF alone. An index is built out of
    /// exactly these two.
    pub fn describe(&self, reference: &str) -> Result<(String, Manifest)> {
        let artifact = self.artifact(reference)?;
        let image = layer(&artifact)?
            .get("digest")
            .and_then(Value::as_str)
            .context("the artifact's layer has no digest")?
            .to_owned();

        let config = artifact
            .get("config")
            .context("not a program: the artifact has no config")?;

        let kind = config
            .get("mediaType")
            .and_then(Value::as_str)
            .unwrap_or("");
        if kind != MANIFEST {
            bail!("not a program: its config is {kind:?}");
        }
        let digest = config
            .get("digest")
            .and_then(Value::as_str)
            .context("the artifact's config has no digest")?;

        let bytes = self.blob(digest)?;
        let json = str::from_utf8(&bytes).context("program manifest is not UTF-8")?;
        Ok((image, Manifest::parse(json)?))
    }

    /// Upload the manifest blob, the image, and the artifact manifest naming
    /// both. Returns the image's digest, which is the layer's.
    pub fn push(&self, reference: &str, elf: &[u8], manifest: &[u8]) -> Result<String> {
        let image = self.upload(elf)?;
        let config = self.upload(manifest)?;

        let artifact = json!({
            "schemaVersion": 2,
            "mediaType": OCI_MANIFEST,
            "artifactType": PROGRAM,
            "config": { "mediaType": MANIFEST, "digest": config, "size": manifest.len() },
            "layers": [ { "mediaType": PROGRAM, "digest": image, "size": elf.len() } ],
            "annotations": annotations(),
        });

        let request = self
            .request(Method::PUT, self.url("manifests", reference))
            .header(CONTENT_TYPE, OCI_MANIFEST)
            .body(serde_json::to_vec(&artifact)?);
        send(request)?;
        Ok(image)
    }

    /// The OCI manifest naming the parts, whatever they turn out to be.
    fn artifact(&self, reference: &str) -> Result<Value> {
        let request = self
            .request(Method::GET, self.url("manifests", reference))
            .header(ACCEPT, format!("{OCI_MANIFEST},{OCI_INDEX}"));
        send(request)?
            .json()
            .context("registry returned a manifest that is not JSON")
    }

    /// One blob, checked against the digest it is addressed by — before the
    /// bytes go anywhere. A registry serving the wrong thing is the case this
    /// exists for, and it is cheap to rule out.
    fn blob(&self, want: &str) -> Result<Vec<u8>> {
        let bytes = send(self.request(Method::GET, self.url("blobs", want)))?
            .bytes()
            .context("cannot read the blob")?
            .to_vec();

        let got = digest(&bytes);
        if got != want {
            bail!("blob does not match its digest: {want} advertised, {got} received");
        }
        Ok(bytes)
    }

    /// One blob, uploaded monolithically: ask where to put it, then put it.
    fn upload(&self, bytes: &[u8]) -> Result<String> {
        let digest = digest(bytes);
        let uploads = format!("{}/v2/{}/blobs/uploads/", self.base, self.repository);
        let opened = send(self.request(Method::POST, uploads))?;

        let location = opened
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .context("registry accepted an upload without saying where to put it")?;
        // The location may be relative, and may already carry the registry's
        // own state in a query string.
        let held = if location.contains('?') { '&' } else { '?' };
        let url = match location.starts_with("http") {
            true => format!("{location}{held}digest={digest}"),
            false => format!("{}{location}{held}digest={digest}", self.base),
        };

        let request = self
            .request(Method::PUT, url)
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(bytes.to_vec());
        send(request)?;
        Ok(digest)
    }

    /// Ask the registry what it wants, then go and get it.
    fn authenticate(&mut self, scope: &str) -> Result<()> {
        let probe = self
            .http
            .get(format!("{}/v2/", self.base))
            .send()
            .with_context(|| format!("cannot reach {}", self.base))?;
        if probe.status() != StatusCode::UNAUTHORIZED {
            return Ok(());
        }

        let challenge = probe
            .headers()
            .get(WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let realm = field(&challenge, "realm")
            .context("registry asked for a token without saying where from")?;

        // Built rather than encoded: a scope is `repository:org/name:pull`,
        // and every character in it is one a query value may carry as itself.
        let service = field(&challenge, "service").unwrap_or_default();
        let mut request = self
            .http
            .get(format!("{realm}?service={service}&scope={scope}"));
        // GHCR reads the token as the password and ignores the user.
        if let Ok(token) = env::var(TOKEN) {
            request = request.basic_auth("berm", Some(token));
        }

        let answer: Value = send(request)?
            .json()
            .context("registry returned a token that is not JSON")?;
        let token = answer
            .get("token")
            .or_else(|| answer.get("access_token"))
            .and_then(Value::as_str)
            .context("registry answered without a token")?;

        self.token = Some(token.to_owned());
        Ok(())
    }

    fn request(&self, method: Method, url: String) -> RequestBuilder {
        let request = self.http.request(method, url);
        match &self.token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    fn url(&self, kind: &str, reference: &str) -> String {
        format!("{}/v2/{}/{kind}/{reference}", self.base, self.repository)
    }
}

/// Turn a refusal into what the registry said, rather than a status code the
/// operator then has to go and look up.
fn send(request: RequestBuilder) -> Result<Response> {
    let response = request.send().context("cannot reach the registry")?;
    if response.status().is_success() {
        return Ok(response);
    }

    let status = response.status();
    let body = response.text().unwrap_or_default();
    match serde_json::from_str::<Value>(&body).ok().and_then(|body| {
        Some(
            body.get("errors")?
                .get(0)?
                .get("message")?
                .as_str()?
                .to_owned(),
        )
    }) {
        Some(message) => bail!("{message}"),
        None => bail!("registry answered {status}"),
    }
}

/// The one layer a program has, refusing anything that is not one.
fn layer(manifest: &Value) -> Result<&Value> {
    // An index is what a container image looks like, and pointing at one is the
    // likely mistake here — worth saying so rather than reporting a missing
    // field.
    if manifest.get("manifests").is_some() {
        bail!("not a program: a multi-platform image, which berm has no use for");
    }

    let layers = manifest
        .get("layers")
        .and_then(Value::as_array)
        .context("not a program: the artifact has no layers")?;
    let [layer] = layers.as_slice() else {
        bail!(
            "not a program: {} layers, where one is the whole of it",
            layers.len()
        );
    };

    let kind = layer.get("mediaType").and_then(Value::as_str).unwrap_or("");
    if kind != PROGRAM {
        bail!("not a program: its layer is {kind:?}");
    }
    Ok(layer)
}

/// Standard OCI keys only. What the program *is* lives in the config blob, and
/// a second copy here would be the one that rots.
fn annotations() -> Value {
    let mut annotations = Map::new();
    if let Ok(repository) = env::var("GITHUB_REPOSITORY") {
        // What makes GHCR show the package against its repository.
        annotations.insert(
            "org.opencontainers.image.source".to_owned(),
            format!("https://github.com/{repository}").into(),
        );
    }
    if let Ok(revision) = env::var("GITHUB_SHA") {
        annotations.insert(
            "org.opencontainers.image.revision".to_owned(),
            revision.into(),
        );
    }
    Value::Object(annotations)
}

/// Plain HTTP for a loopback registry, which is the only place one runs
/// without TLS.
fn loopback(registry: &str) -> bool {
    let host = registry.split(':').next().unwrap_or(registry);
    host == "localhost" || host == "127.0.0.1"
}

/// Pull `key="…"` out of a `WWW-Authenticate` challenge.
fn field(challenge: &str, key: &str) -> Option<String> {
    let at = challenge.find(&format!("{key}=\""))? + key.len() + 2;
    let rest = &challenge[at..];
    Some(rest[..rest.find('"')?].to_owned())
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}
