//! Reddit's RSS feeds on `www.reddit.com`, through the host's proxy: a
//! subreddit's listing and a post's comments. Reddit answers `429` past about
//! one request a minute from one address.

use crate::sort::Sort;
use alloc::{format, string::String};

const BASE: &str = "https://www.reddit.com";
/// Reddit answers `403` to a client that does not present as a browser.
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/120 Safari/537.36";
/// Posts per listing; Reddit caps it at 100.
const POSTS: u32 = 25;
/// Comments per post, in Reddit's top order.
pub const COMMENTS: u32 = 100;

/// The listing of `sub` ranked by `sort`.
pub fn listing(sub: &str, sort: Sort) -> Result<String, String> {
    let window = match sort.window {
        Some(window) => format!("&t={window}"),
        None => String::new(),
    };
    get(&format!(
        "{BASE}/r/{sub}/{}.rss?limit={POSTS}{window}",
        sort.segment
    ))
}

/// The comments under post `id` (without its `t3_`) in `sub`.
pub fn comments(sub: &str, id: &str) -> Result<String, String> {
    get(&format!(
        "{BASE}/r/{sub}/comments/{id}/.rss?sort=top&limit={COMMENTS}"
    ))
}

fn get(url: &str) -> Result<String, String> {
    let response = berm_lang::http::proxy("GET", url, &[("User-Agent", USER_AGENT)], b"")
        .map_err(|error| format!("{url}: {}", error.message()))?;
    if response.status != 200 {
        return Err(format!("{url}: status {}", response.status));
    }
    String::from_utf8(response.body).map_err(|_| format!("{url}: not UTF-8"))
}
