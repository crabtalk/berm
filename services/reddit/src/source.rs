//! Reddit as a lake source: a feed is a subreddit's listing, keyed
//! `{sub}/{sort}`.

use crate::{
    reddit,
    sort::Sort,
    tools::{Comment, Comments, Post},
};
use alloc::{format, string::String, vec::Vec};
use berm_lang::{CallError, html};
use lake::{
    Feed, Listed, Schedule, Source, Watch,
    rss::{self, Entry},
};

pub struct Reddit;

impl Source for Reddit {
    const NAME: &'static str = "reddit";
    const SCHEDULE: Schedule = Schedule {
        tick: 5 * 60 * 1000,
        refetch: 60 * 60 * 1000,
        feeds: 10,
        threads: 10,
        rewarm: 6 * 60 * 60 * 1000,
        window: 48 * 60 * 60 * 1000,
    };
    type Item = Entry;
    type Thread = Vec<Comment>;

    fn fetch(feed: &Feed) -> Result<Vec<Listed<Entry>>, String> {
        let (sub, sort) = feed
            .key
            .split_once('/')
            .and_then(|(sub, sort)| Some((sub, Sort::parse(sort)?)))
            .ok_or_else(|| format!("{} is not a listing", feed.key))?;
        let entries = rss::entries(&reddit::listing(sub, sort)?)?;
        Ok(entries
            .into_iter()
            .filter_map(|entry| {
                Some(Listed {
                    id: String::from(entry.id.strip_prefix("t3_")?),
                    posted: entry.posted?,
                    item: entry,
                })
            })
            .collect())
    }

    fn post(feed: &Feed, entry: Entry) -> Result<usize, CallError> {
        let (sub, sort) = feed.key.split_once('/').unwrap_or((&feed.key, ""));
        let content = entry.content.unwrap_or_default();
        berm_lang::emit!(Post {
            id: String::from(entry.id.strip_prefix("t3_").unwrap_or(&entry.id)),
            sub: String::from(sub),
            sort: String::from(sort),
            url: linked(&content).filter(|url| *url != entry.link),
            text: selftext(&content),
            title: entry.title,
            author: entry.author.map(|author| user(&author)),
            posted: entry.posted.unwrap_or_default(),
            thumbnail: entry.image,
        })
    }

    fn watches(_feed: &Feed) -> bool {
        true
    }

    fn thread(watch: &Watch) -> Result<(u64, Vec<Comment>), String> {
        let sub = sub(watch);
        let comments: Vec<Comment> = rss::entries(&reddit::comments(sub, &watch.id)?)?
            .into_iter()
            .filter_map(|entry| {
                let id = entry.id.strip_prefix("t1_")?;
                let text = html::text(&entry.content?);
                if text.is_empty() || text == "[deleted]" || text == "[removed]" {
                    return None;
                }
                Some(Comment {
                    id: String::from(id),
                    author: entry.author.map(|author| user(&author)),
                    text,
                    posted: entry.posted?,
                })
            })
            .collect();
        Ok((comments.len() as u64, comments))
    }

    fn reply(watch: &Watch, comments: Vec<Comment>) -> Result<usize, CallError> {
        berm_lang::emit!(Comments {
            post: watch.id.clone(),
            sub: String::from(sub(watch)),
            comments,
        })
    }
}

/// The subreddit of the feed that first showed `watch`.
fn sub(watch: &Watch) -> &str {
    watch
        .feed
        .split_once('/')
        .map_or(watch.feed.as_str(), |(sub, _)| sub)
}

/// A post's own text: what its content holds between `<!-- SC_OFF -->` and
/// `<!-- SC_ON -->`.
fn selftext(content: &str) -> Option<String> {
    let (_, rest) = content.split_once("<!-- SC_OFF -->")?;
    let (body, _) = rest.split_once("<!-- SC_ON -->")?;
    Some(html::text(body)).filter(|text| !text.is_empty())
}

/// The href of the `[link]` anchor in a post's content.
fn linked(content: &str) -> Option<String> {
    let (before, _) = content.rsplit_once(">[link]</a>")?;
    let (_, href) = before.rsplit_once("href=\"")?;
    let (href, _) = href.split_once('"')?;
    Some(html::unescape(href))
}

/// A username without its `/u/`.
fn user(author: &str) -> String {
    String::from(author.trim().trim_start_matches("/u/"))
}
