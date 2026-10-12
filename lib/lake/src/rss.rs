//! The entries of an RSS 2.0, RSS 1.0 or Atom feed.
//!
//! Elements are matched by their prefix as written (`dc:`, `content:`,
//! `media:`), not by namespace.

use crate::time;
use alloc::{format, string::String, vec::Vec};
use berm_lang::html;
use xmlparser::{ElementEnd, Token, Tokenizer};

/// One `<item>` or `<entry>`. Text is unescaped once, so `summary` and
/// `content` are the HTML the feed carries.
#[derive(Default)]
pub struct Entry {
    /// `guid`, Atom's `id` or RSS 1.0's `rdf:about`.
    pub id: String,
    pub title: String,
    /// `link`, or the `href` of Atom's first `link` with no `rel` or
    /// `rel="alternate"`.
    pub link: String,
    /// `dc:creator`, RSS's `author`, or the `name` of Atom's `author`.
    pub author: Option<String>,
    /// `description` or Atom's `summary`.
    pub summary: Option<String>,
    /// `content:encoded` or Atom's `content`.
    pub content: Option<String>,
    /// `pubDate`, `published` or `dc:date`, or `updated` when there is none.
    pub posted: Option<u64>,
    /// The first of `media:thumbnail`, an image `media:content`, or an image
    /// `enclosure`.
    pub image: Option<String>,
}

pub fn entries(xml: &str) -> Result<Vec<Entry>, String> {
    let xml = xml.trim_start_matches('\u{feff}').trim_start();
    let mut entries = Vec::new();
    let mut entry: Option<Entry> = None;
    let mut updated = None;
    // The open elements, outermost first, and the depth of the entry's.
    let mut open: Vec<(&str, &str)> = Vec::new();
    let mut depth = 0;
    // The element whose start tag is being read, and its attributes.
    let mut start = ("", "");
    let mut attrs: Vec<(&str, &str, String)> = Vec::new();
    // The text since an entry's child, or an element outside an entry,
    // started. A child's own elements add to it.
    let mut text = String::new();
    for token in Tokenizer::from(xml) {
        let token = token.map_err(|error| format!("feed: {error}"))?;
        match token {
            Token::ElementStart { prefix, local, .. } => {
                start = (prefix.as_str(), local.as_str());
                attrs.clear();
                if entry.is_none() || open.len() <= depth {
                    text.clear();
                }
            }
            Token::Attribute {
                prefix,
                local,
                value,
                ..
            } => attrs.push((
                prefix.as_str(),
                local.as_str(),
                html::unescape(value.as_str()),
            )),
            Token::Text { text: part } => text.push_str(&html::unescape(part.as_str())),
            Token::Cdata { text: part, .. } => text.push_str(part.as_str()),
            Token::ElementEnd { end, .. } => {
                let close = match end {
                    ElementEnd::Open => {
                        open.push(start);
                        None
                    }
                    ElementEnd::Empty => Some(start),
                    ElementEnd::Close(..) => open.pop(),
                };
                let entering = matches!(end, ElementEnd::Open)
                    && entry.is_none()
                    && matches!(start, ("", "item" | "entry"));
                if entering {
                    entry = Some(Entry {
                        id: attr(&attrs, "rdf", "about")
                            .map(String::from)
                            .unwrap_or_default(),
                        ..Entry::default()
                    });
                    updated = None;
                    depth = open.len();
                    continue;
                }
                let Some(current) = entry.as_mut() else {
                    continue;
                };
                if !matches!(end, ElementEnd::Close(..)) {
                    opened(current, start, &attrs);
                }
                let Some(element) = close else { continue };
                match open.len() + 1 - depth {
                    0 => {
                        let mut done = entry.take().unwrap_or_default();
                        done.posted = done.posted.or(updated);
                        entries.push(done);
                    }
                    child => {
                        let parent = open.last().copied().unwrap_or_default();
                        closed(current, &mut updated, element, parent, child, &text);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(entries)
}

/// Read what `element`'s start tag carries into `entry`.
fn opened(entry: &mut Entry, element: (&str, &str), attrs: &[(&str, &str, String)]) {
    let url = attr(attrs, "", "url");
    let kind = attr(attrs, "", "type");
    let image = |kind: Option<&str>| kind.is_none_or(|kind| kind.starts_with("image/"));
    match element {
        ("", "link") if entry.link.is_empty() => {
            let rel = attr(attrs, "", "rel");
            if rel.is_none_or(|rel| rel == "alternate")
                && let Some(href) = attr(attrs, "", "href")
            {
                entry.link = String::from(href);
            }
        }
        ("media", "thumbnail") => {
            entry.image = entry.image.take().or(url.map(String::from));
        }
        ("media", "content") => {
            let medium = attr(attrs, "", "medium");
            if medium.map_or(image(kind), |medium| medium == "image") {
                entry.image = entry.image.take().or(url.map(String::from));
            }
        }
        ("", "enclosure") if kind.is_some() && image(kind) => {
            entry.image = entry.image.take().or(url.map(String::from));
        }
        _ => {}
    }
}

/// Read `element`'s text into `entry`. `child` is its depth below the entry,
/// 1 for the entry's own child, and `parent` the element it sits in.
fn closed(
    entry: &mut Entry,
    updated: &mut Option<u64>,
    element: (&str, &str),
    parent: (&str, &str),
    child: usize,
    text: &str,
) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let owned = || Some(String::from(text));
    match (child, element) {
        (1, ("", "title")) => entry.title = String::from(text),
        (1, ("", "link")) if entry.link.is_empty() => entry.link = String::from(text),
        (1, ("", "guid" | "id")) => entry.id = String::from(text),
        (1, ("dc", "creator") | ("", "author")) => {
            entry.author = entry.author.take().or_else(owned)
        }
        (2, ("", "name")) if parent == ("", "author") => {
            entry.author = entry.author.take().or_else(owned)
        }
        (1, ("", "description" | "summary")) => entry.summary = owned(),
        (1, ("content", "encoded") | ("", "content")) => entry.content = owned(),
        (1, ("", "pubDate" | "published") | ("dc", "date")) => {
            entry.posted = entry.posted.or(date(text))
        }
        (1, ("", "updated")) => *updated = date(text),
        _ => {}
    }
}

/// `text` as an RFC 3339 or an RFC 822 time.
fn date(text: &str) -> Option<u64> {
    match text.as_bytes().get(4) {
        Some(b'-') => time::rfc3339(text),
        _ => time::rfc822(text),
    }
}

fn attr<'a>(attrs: &'a [(&str, &str, String)], prefix: &str, local: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(p, l, _)| *p == prefix && *l == local)
        .map(|(_, _, value)| value.as_str())
}
