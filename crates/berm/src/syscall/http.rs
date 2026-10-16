//! `berm.http.fetch` and `berm.http.proxy` — one HTTP request, to a host the
//! program's manifest names under the syscall it makes.
//!
//! berm checks the request against the asking program's [`crate::Deps`] and
//! substitutes every `{secret:NAME}` in a header value. Resolving a secret and
//! sending the request are the host's, and arrive as the two closures below.

use crate::{Callsite, Meter, Refused, Syscall, abi, wire};
use anyhow::{Result, bail};
use std::{future::Future, sync::Arc};

/// What opens a secret's name in a header value.
const SECRET: &str = "{secret:";

/// One request, as the host is handed it: allowed by the manifest, and every
/// secret already substituted.
#[derive(Debug)]
pub struct Request {
    pub method: String,
    pub url: String,
    /// The host `url` names, lowercased, without port or credentials.
    pub host: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// What the host got back.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Serve `berm.http.fetch`.
///
/// `secret` is handed the callsite, a secret's name and the request's host,
/// and answers the value granted for that triple or `None`. `send` performs
/// the request; it is not handed one the manifest does not allow. A guest
/// suspended on `send` cannot notice its deadline, so `send` bounds itself by
/// [`Callsite::deadline`] and does not follow redirects. In a dry chain
/// ([`Callsite::dry`]), a request one of its answers matches is answered with it
/// and not handed to `send`, and with `offline` one none matches is refused.
pub fn fetch<S, Sent, R, Found>(send: S, secret: R) -> Syscall
where
    S: Fn(Callsite, Request) -> Sent + Send + Sync + 'static,
    Sent: Future<Output = Result<Response>> + Send + 'static,
    R: Fn(Callsite, String, String) -> Found + Send + Sync + 'static,
    Found: Future<Output = Result<Option<String>>> + Send + 'static,
{
    serve(abi::HTTP_FETCH, send, secret)
}

/// Serve `berm.http.proxy`: [`fetch`], to the hosts the manifest names under
/// it, where `send` goes through the host's proxy. Each answered request meters
/// [`Meter::proxied`]: its URL, header names and values and body, and the
/// response's header names and values and body.
pub fn proxy<S, Sent, R, Found>(send: S, secret: R) -> Syscall
where
    S: Fn(Callsite, Request) -> Sent + Send + Sync + 'static,
    Sent: Future<Output = Result<Response>> + Send + 'static,
    R: Fn(Callsite, String, String) -> Found + Send + Sync + 'static,
    Found: Future<Output = Result<Option<String>>> + Send + 'static,
{
    serve(abi::HTTP_PROXY, send, secret)
}

fn serve<S, Sent, R, Found>(name: &'static str, send: S, secret: R) -> Syscall
where
    S: Fn(Callsite, Request) -> Sent + Send + Sync + 'static,
    Sent: Future<Output = Result<Response>> + Send + 'static,
    R: Fn(Callsite, String, String) -> Found + Send + Sync + 'static,
    Found: Future<Output = Result<Option<String>>> + Send + 'static,
{
    let (send, secret) = (Arc::new(send), Arc::new(secret));
    Syscall {
        name: name.to_owned(),
        call: Arc::new(move |at: Callsite, request: Vec<u8>| {
            let (send, secret) = (send.clone(), secret.clone());
            Box::pin(async move {
                let mut request = read(&request)?;
                permitted(&at, name, &request.host)?;
                for at_header in 0..request.headers.len() {
                    let value = &request.headers[at_header].1;
                    if !value.contains(SECRET) {
                        continue;
                    }
                    let mut substituted = String::with_capacity(value.len());
                    let mut rest = value.as_str();
                    while let Some(start) = rest.find(SECRET) {
                        let after = &rest[start + SECRET.len()..];
                        let Some(end) = after.find('}') else {
                            bail!("a header opens {SECRET} and never closes it");
                        };
                        let name = &after[..end];
                        if !at.deps.secrets.iter().any(|declared| declared == name) {
                            return Err(Refused(format!(
                                "{} does not declare the secret {name:?}",
                                at.program
                            ))
                            .into());
                        }
                        let found =
                            secret(at.clone(), name.to_owned(), request.host.clone()).await?;
                        let Some(found) = found else {
                            return Err(Refused(format!(
                                "no secret {name:?} is granted to {} for {}",
                                at.program, request.host
                            ))
                            .into());
                        };
                        substituted.push_str(&rest[..start]);
                        substituted.push_str(&found);
                        rest = &after[end + 1..];
                    }
                    substituted.push_str(rest);
                    request.headers[at_header].1 = substituted;
                }
                let sent = request.url.len() + request.body.len() + headers(&request.headers);
                let answered = at
                    .dry
                    .as_deref()
                    .map(|dry| (dry.offline, dry.answer(&request.method, &request.url)));
                let response = match answered {
                    Some((_, Some(answer))) => Response {
                        status: answer.status,
                        headers: answer.headers.clone(),
                        body: answer.body.clone().into_bytes(),
                    },
                    Some((true, None)) => {
                        return Err(Refused(format!(
                            "this dry call answers no {} {} and sends nothing",
                            request.method, request.url
                        ))
                        .into());
                    }
                    _ => send(at.clone(), request).await?,
                };
                if name == abi::HTTP_PROXY {
                    at.meter(Meter {
                        proxied: (sent + response.body.len() + headers(&response.headers)) as u64,
                        ..Meter::default()
                    });
                }
                Ok(write(&response))
            })
        }),
    }
}

/// The bytes of `headers`' names and values.
fn headers(headers: &[(String, String)]) -> usize {
    headers
        .iter()
        .map(|(name, value)| name.len() + value.len())
        .sum()
}

/// Refuse a host the asking program's manifest does not name under `syscall`.
fn permitted(at: &Callsite, syscall: &str, host: &str) -> Result<()> {
    let declared = at.deps.syscalls.get(syscall);
    if !declared.is_some_and(|hosts| {
        hosts
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(host))
    }) {
        return Err(Refused(format!(
            "{} does not declare {host} under {syscall}",
            at.program
        ))
        .into());
    }
    Ok(())
}

fn read(request: &[u8]) -> Result<Request> {
    let fields = wire::fields(request)?;
    let method = wire::text(&fields, 0, "method")?.to_owned();
    let url = wire::text(&fields, 1, "url")?.to_owned();
    let Some(body) = fields.get(2) else {
        bail!("request has no body");
    };
    let trailing = &fields[3..];
    if trailing.len() % 2 != 0 {
        bail!("a header has a name and no value");
    }
    let mut headers = Vec::with_capacity(trailing.len() / 2);
    for pair in trailing.chunks(2) {
        headers.push((
            std::str::from_utf8(pair[0])?.to_owned(),
            std::str::from_utf8(pair[1])?.to_owned(),
        ));
    }
    let Some(host) = host(&url) else {
        bail!("{url} is not an http or https URL with a host");
    };
    Ok(Request {
        method,
        host: host.to_ascii_lowercase(),
        url,
        headers,
        body: body.to_vec(),
    })
}

fn write(response: &Response) -> Vec<u8> {
    let status = response.status.to_string();
    let mut fields: Vec<&[u8]> = vec![status.as_bytes(), &response.body];
    for (name, value) in &response.headers {
        fields.push(name.as_bytes());
        fields.push(value.as_bytes());
    }
    wire::frame(&fields)
}

/// The host an `http` or `https` URL names, without port or credentials.
fn host(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    let host = match authority.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next()?,
        None => authority.split(':').next()?,
    };
    (!host.is_empty()).then_some(host)
}
