//! A desk read from its index page.

use crate::web;
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use berm_lang::html;
use lake::{Feed, rss::Entry, time};

/// Articles fetched per fetch of an index; the rest are listed on later
/// fetches.
const ARTICLES: usize = 10;

/// The articles `index` lists under `prefix`, in page order. An article
/// `feed` showed is listed by its link and date alone; one it did not is
/// fetched for its `og:title`, `og:description` and `og:image`, and left out
/// when that fails or past [`ARTICLES`] of them.
pub fn entries(feed: &Feed, index: &str, prefix: &str) -> Result<Vec<Entry>, String> {
    let page = web::get(index)?;
    let mut entries = Vec::new();
    let mut fetched = 0;
    for (link, posted) in articles(&page, index, prefix) {
        if feed.shown.contains(&link) {
            entries.push(Entry {
                link,
                posted: Some(posted),
                ..Entry::default()
            });
            continue;
        }
        if fetched == ARTICLES {
            continue;
        }
        fetched += 1;
        let article = match web::get(&link) {
            Ok(article) => article,
            Err(error) => {
                berm_lang::trace(&error);
                continue;
            }
        };
        let Some(title) = meta(&article, "og:title") else {
            berm_lang::trace(&format!("{link}: no og:title"));
            continue;
        };
        entries.push(Entry {
            id: link.clone(),
            title,
            summary: meta(&article, "og:description"),
            image: meta(&article, "og:image"),
            link,
            posted: Some(posted),
            ..Entry::default()
        });
    }
    Ok(entries)
}

/// The absolute links on `html` starting with `prefix`, each once, with the
/// date of the `<time>` inside its anchor. A link with no such date is left
/// out.
fn articles(html: &str, page: &str, prefix: &str) -> Vec<(String, u64)> {
    let lower = html.to_ascii_lowercase();
    let mut out: Vec<(String, u64)> = Vec::new();
    let mut at = 0;
    while let Some(found) = lower[at..].find("href=") {
        let start = at + found + "href=".len();
        at = start;
        let rest = &html[start..];
        let href = match rest.chars().next() {
            Some(quote @ ('"' | '\'')) => rest[1..].split(quote).next().unwrap_or(""),
            _ => rest
                .split([' ', '\t', '\n', '\r', '>'])
                .next()
                .unwrap_or(""),
        };
        if !href.starts_with(prefix) || href.len() <= prefix.len() {
            continue;
        }
        let link = absolute(page, href);
        if out.iter().any(|(seen, _)| *seen == link) {
            continue;
        }
        if let Some(posted) = published(rest, &lower[start..]) {
            out.push((link, posted));
        }
    }
    out
}

/// The date of the `<time>` inside the anchor whose `href` value `html` and
/// its lowercase `lower` start at, spelled `Oct 9, 2026`, at midnight UTC.
fn published(html: &str, lower: &str) -> Option<u64> {
    let close = lower.find("</a")?;
    let open = lower[..close].find("<time")?;
    let text = open + tag_end(&lower[open..close]) + 1;
    let day = html::unescape(html.get(text..close)?.split('<').next()?);
    let mut parts = day.split([' ', ',']).filter(|part| !part.is_empty());
    let month = time::month(parts.next()?)?;
    let day = parts.next()?.parse().ok()?;
    let year = parts.next()?.parse().ok()?;
    time::date(year, month, day)
}

/// The `content` of the `<meta>` whose `property` or `name` is `property`,
/// unescaped, its whitespace collapsed.
fn meta(html: &str, property: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let needle = format!("\"{property}\"");
    let mut at = 0;
    while let Some(found) = lower[at..].find("<meta") {
        let start = at + found;
        let end = start + tag_end(&lower[start..]);
        at = end;
        if !lower[start..end].contains(&needle) {
            continue;
        }
        let content = attr(&html[start..end], "content")?;
        let content = html::unescape(content);
        let words: Vec<&str> = content.split_whitespace().collect();
        if !words.is_empty() {
            return Some(words.join(" "));
        }
    }
    None
}

/// A tag's attribute value, quoted or bare.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let at = tag.to_ascii_lowercase().find(&format!("{name}="))? + name.len() + 1;
    let rest = &tag[at..];
    match rest.chars().next()? {
        quote @ ('"' | '\'') => rest[1..].split(quote).next(),
        _ => rest.split([' ', '\t', '\n', '\r', '>']).next(),
    }
}

/// The offset of the `>` that ends the tag `s` starts with, outside quoted
/// values; `s.len()` when there is none.
fn tag_end(s: &str) -> usize {
    let mut quote = None;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return i,
            _ => {}
        }
    }
    s.len()
}

/// `href` resolved against the URL of the page it is on.
fn absolute(page: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        href.to_string()
    } else if let Some(rest) = href.strip_prefix("//") {
        format!("https://{rest}")
    } else if let Some(rest) = href.strip_prefix('/') {
        let after_scheme = page.find("://").map_or(0, |i| i + 3);
        let host = page[after_scheme..].split('/').next().unwrap_or("");
        format!("https://{host}/{rest}")
    } else {
        format!("{}/{href}", page.trim_end_matches('/'))
    }
}
