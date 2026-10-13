//! Hacker News as a lake source: a feed is a lane, keyed by its name.

use crate::{Comment, Story, Thread, algolia, lane::Lane};
use alloc::{format, string::String, vec::Vec};
use berm_lang::{CallError, html};
use lake::{Feed, Listed, Schedule, Source, Watch};

/// Top-level comments a `Thread` carries.
const TOP_LEVEL: usize = 100;
/// Reply levels a `Thread` carries, top level included.
const MAX_DEPTH: usize = 6;

pub struct Hn;

impl Source for Hn {
    const NAME: &'static str = "hn";
    const SCHEDULE: Schedule = Schedule {
        tick: 5 * 60 * 1000,
        refetch: 5 * 60 * 1000,
        feeds: Lane::ALL.len(),
        threads: 30,
        rewarm: 2 * 60 * 60 * 1000,
        window: 48 * 60 * 60 * 1000,
    };
    type Item = algolia::Hit;
    type Thread = (Option<u64>, Vec<Comment>);

    fn fetch(feed: &Feed) -> Result<Vec<Listed<algolia::Hit>>, String> {
        let lane = Lane::named(&feed.key).ok_or_else(|| format!("no lane {}", feed.key))?;
        Ok(algolia::search(lane)?
            .hits
            .into_iter()
            .filter(|hit| hit.title.is_some() && hit.object_id.parse::<u64>().is_ok())
            .map(|hit| Listed {
                id: hit.object_id.clone(),
                posted: hit.created_at_i * 1000,
                item: hit,
            })
            .collect())
    }

    fn post(feed: &Feed, hit: algolia::Hit) -> Result<usize, CallError> {
        berm_lang::emit!(Story {
            id: hit.object_id.parse().unwrap_or_default(),
            lane: feed.key.clone(),
            title: hit.title.unwrap_or_default(),
            url: hit.url,
            text: hit
                .story_text
                .as_deref()
                .map(html::text)
                .filter(|text| !text.is_empty()),
            author: hit.author,
            posted: hit.created_at_i * 1000,
            points: hit.points,
            comments: hit.num_comments,
        })
    }

    fn watches(feed: &Feed) -> bool {
        Lane::named(&feed.key).is_some_and(Lane::warmed)
    }

    fn thread(watch: &Watch) -> Result<(u64, Self::Thread), String> {
        let id = watch
            .id
            .parse()
            .map_err(|_| format!("{} is not a story id", watch.id))?;
        let story = algolia::item(id)?;
        let mut comments = Vec::new();
        let mut top = 0;
        for child in story.children {
            if top == TOP_LEVEL {
                break;
            }
            let before = comments.len();
            push(child, story.id, 0, &mut comments);
            if comments.len() > before {
                top += 1;
            }
        }
        Ok((comments.len() as u64, (story.points, comments)))
    }

    fn reply(watch: &Watch, (points, comments): Self::Thread) -> Result<usize, CallError> {
        berm_lang::emit!(Thread {
            story: watch.id.parse().unwrap_or_default(),
            points,
            comments,
        })
    }
}

/// Appends `item` and its replies under `parent`, depth-first, leaving out an
/// empty comment with no replies left.
fn push(item: algolia::Item, parent: u64, depth: usize, comments: &mut Vec<Comment>) {
    if depth >= MAX_DEPTH {
        return;
    }
    let at = comments.len();
    comments.push(Comment {
        id: item.id,
        parent,
        author: item.author,
        text: item.text.as_deref().map(html::text).unwrap_or_default(),
        posted: item.created_at_i * 1000,
    });
    for child in item.children {
        push(child, item.id, depth + 1, comments);
    }
    if comments.len() == at + 1 && comments[at].text.trim().is_empty() {
        comments.pop();
    }
}
