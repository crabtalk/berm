//! What both berm services have in common: they refuse in JSON.

use anyhow::{Context, Result, bail};
use berm_api::Failed;
use reqwest::blocking::Response;

/// Turn a refusal into the message the service sent, rather than a status code
/// the operator then has to go and look up.
pub fn read(host: &str, response: reqwest::Result<Response>) -> Result<Response> {
    let response = response.with_context(|| format!("cannot reach {host}"))?;
    if response.status().is_success() {
        return Ok(response);
    }

    let status = response.status();
    match response.json::<Failed>() {
        Ok(failed) => bail!("{}", failed.error),
        Err(_) => bail!("{host} answered {status}"),
    }
}
