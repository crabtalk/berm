//! One HTTP request, to a host this program's manifest declares under
//! `berm.http.fetch`, or under `berm.http.proxy` to send it through the host's
//! proxy.
//!
//! A header value may carry `{secret:NAME}` for a secret the manifest
//! declares. The host substitutes it, and this program never holds the value.

use crate::abi::{self, host, host::CallError, wire};
use alloc::{string::String, vec::Vec};

/// What the far end answered.
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Make one request and wait for its answer.
///
/// ```ignore
/// let page = berm_lang::http::fetch(
///     "GET",
///     "https://api.example.com/items",
///     &[("Authorization", "Bearer {secret:example}")],
///     b"",
/// )?;
/// ```
///
/// [`CallError::Refused`] means nothing was sent: a host or secret the
/// manifest does not declare, or a secret nobody granted. A status the far end
/// answered with, 4xx and 5xx included, is a [`Response`].
pub fn fetch(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Result<Response, CallError> {
    send(abi::HOST_HTTP_FETCH, method, url, headers, body)
}

/// [`fetch`], through the host's proxy. The host meters the bytes sent and
/// received against this program as `proxied`.
pub fn proxy(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Result<Response, CallError> {
    send(abi::HOST_HTTP_PROXY, method, url, headers, body)
}

fn send(
    number: u64,
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Result<Response, CallError> {
    let mut request = wire::request(&[method.as_bytes(), url.as_bytes(), body]);
    for (name, value) in headers {
        wire::field(&mut request, name.as_bytes());
        wire::field(&mut request, value.as_bytes());
    }
    let reply = host::call(number, &request)?;

    let unreadable =
        || CallError::Failed(String::from("host framed a reply this guest cannot read"));
    let fields = wire::fields(&reply).ok_or_else(unreadable)?;
    let [status, body, rest @ ..] = fields.as_slice() else {
        return Err(unreadable());
    };
    if rest.len() % 2 != 0 {
        return Err(unreadable());
    }
    let status = core::str::from_utf8(status)
        .ok()
        .and_then(|status| status.parse().ok())
        .ok_or_else(unreadable)?;
    let headers = rest
        .chunks(2)
        .map(|pair| {
            (
                String::from_utf8_lossy(pair[0]).into_owned(),
                String::from_utf8_lossy(pair[1]).into_owned(),
            )
        })
        .collect();
    Ok(Response {
        status,
        headers,
        body: body.to_vec(),
    })
}
