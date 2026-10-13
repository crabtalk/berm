//! News as a lake source: a feed is a desk, keyed by [`Desk::key`].

use crate::{
    Article,
    catalog::{Desk, Reader},
    page, web,
};
use alloc::{format, string::String, vec::Vec};
use berm_lang::{CallError, html};
use lake::{
    Feed, Listed, Schedule, Source, Watch,
    rss::{self, Entry},
};

pub struct News;

impl Source for News {
    const NAME: &'static str = "news";
    const SCHEDULE: Schedule = Schedule {
        tick: 5 * 60 * 1000,
        refetch: 2 * 60 * 60 * 1000,
        feeds: 10,
        threads: 0,
        rewarm: 0,
        window: 0,
    };
    type Item = Entry;
    type Thread = ();

    fn fetch(feed: &Feed) -> Result<Vec<Listed<Entry>>, String> {
        let desk = Desk::find(&feed.key).ok_or_else(|| format!("no desk {}", feed.key))?;
        let entries = match desk.reader {
            Reader::Feed(url) => rss::entries(&web::get(url)?)?,
            Reader::Page { index, link_prefix } => page::entries(feed, index, link_prefix)?,
        };
        Ok(entries
            .into_iter()
            .filter(|entry| !entry.link.is_empty())
            .filter_map(|entry| {
                Some(Listed {
                    id: entry.link.clone(),
                    posted: entry.posted?,
                    item: entry,
                })
            })
            .collect())
    }

    fn post(feed: &Feed, entry: Entry) -> Result<usize, CallError> {
        let text = |html: Option<String>| {
            html.map(|html| html::text(&html))
                .filter(|text| !text.is_empty())
        };
        berm_lang::emit!(Article {
            desk: feed.key.clone(),
            title: entry.title,
            url: entry.link,
            summary: text(entry.summary),
            content: text(entry.content),
            author: entry.author,
            posted: entry.posted.unwrap_or_default(),
            image: entry.image,
        })
    }

    fn watches(_feed: &Feed) -> bool {
        false
    }

    fn thread(watch: &Watch) -> Result<(u64, ()), String> {
        Err(format!("{} has no thread", watch.id))
    }

    fn reply(_watch: &Watch, _thread: ()) -> Result<usize, CallError> {
        Ok(0)
    }
}
