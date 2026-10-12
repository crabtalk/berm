//! Feeds and pages over `berm.http.fetch`.

use alloc::{
    format,
    string::{String, ToString},
};

/// Some outlets answer `403` or a redirect to a client that does not present
/// as a browser.
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/120 Safari/537.36";

/// The body `url` answers with `200`, invalid UTF-8 replaced.
pub fn get(url: &str) -> Result<String, String> {
    let response = berm_lang::http::fetch("GET", url, &[("User-Agent", USER_AGENT)], b"")
        .map_err(|error| format!("{url}: {}", error.message()))?;
    if response.status != 200 {
        return Err(format!("{url}: status {}", response.status));
    }
    Ok(String::from_utf8_lossy(&response.body).to_string())
}
