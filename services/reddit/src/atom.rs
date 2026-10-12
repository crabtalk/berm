//! The entries of a Reddit Atom feed.

use alloc::{format, string::String, vec::Vec};
use berm_lang::html;
use xmlparser::{ElementEnd, Token, Tokenizer};

/// One `<entry>`. Text is unescaped once, so `content` is the entry's HTML.
#[derive(Default)]
pub struct Entry {
    /// `t3_…` for a post, `t1_…` for a comment.
    pub id: String,
    pub title: String,
    pub link: String,
    pub author: Option<String>,
    pub content: Option<String>,
    /// `published`, or `updated` when there is none, in Unix milliseconds.
    pub posted: Option<u64>,
    pub thumbnail: Option<String>,
}

pub fn entries(xml: &str) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    let mut entry: Option<Entry> = None;
    let mut element = "";
    let mut in_author = false;
    for token in Tokenizer::from(xml) {
        let token = token.map_err(|error| format!("feed: {error}"))?;
        let Some(open) = entry.as_mut() else {
            if let Token::ElementStart { local, .. } = token
                && local.as_str() == "entry"
            {
                entry = Some(Entry::default());
            }
            continue;
        };
        match token {
            Token::ElementStart { local, .. } => {
                element = local.as_str();
                if element == "author" {
                    in_author = true;
                }
            }
            Token::Attribute { local, value, .. } => match (element, local.as_str()) {
                ("link", "href") if open.link.is_empty() => {
                    open.link = html::unescape(value.as_str())
                }
                ("thumbnail", "url") => open.thumbnail = Some(html::unescape(value.as_str())),
                _ => {}
            },
            Token::Text { text } | Token::Cdata { text, .. } => {
                let text = text.as_str();
                match element {
                    "id" => open.id = html::unescape(text),
                    "title" => open.title = html::unescape(text),
                    "name" if in_author => open.author = Some(html::unescape(text)),
                    "content" => open.content = Some(html::unescape(text)),
                    "published" => open.posted = millis(text).or(open.posted),
                    "updated" if open.posted.is_none() => open.posted = millis(text),
                    _ => {}
                }
            }
            Token::ElementEnd { end, .. } => match end {
                ElementEnd::Open => {}
                ElementEnd::Empty => element = "",
                ElementEnd::Close(_, local) => {
                    match local.as_str() {
                        "entry" => entries.extend(entry.take()),
                        "author" => in_author = false,
                        _ => {}
                    }
                    element = "";
                }
            },
            _ => {}
        }
    }
    Ok(entries)
}

/// An RFC 3339 time, `2026-10-11T06:55:00+00:00`, in Unix milliseconds.
fn millis(text: &str) -> Option<u64> {
    let text = text.trim();
    let digits = |range: core::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (digits(0..4)?, digits(5..7)?, digits(8..10)?);
    let (hour, minute, second) = (digits(11..13)?, digits(14..16)?, digits(17..19)?);
    let zone = text[19..].trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let offset = match zone {
        "Z" | "z" => 0,
        _ => {
            let sign = match zone.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let hours: i64 = zone.get(1..3)?.parse().ok()?;
            let minutes: i64 = zone.get(4..6)?.parse().ok()?;
            sign * (hours * 3600 + minutes * 60)
        }
    };
    // Days from 1970-01-01 to the civil date, after Howard Hinnant's
    // `days_from_civil`.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let year_of_era = y - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    let seconds = days * 86_400 + hour * 3600 + minute * 60 + second - offset;
    u64::try_from(seconds).ok().map(|seconds| seconds * 1000)
}
