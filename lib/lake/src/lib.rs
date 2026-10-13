//! The runtime of a source service: the feeds it follows, the items they show
//! and the threads under them, fetched on the service's own wake and emitted
//! as logs.
//!
//! A tick fetches the feeds due, least recently fetched first, and logs each
//! item new to one. It then fetches the threads due, least recently fetched
//! first, and logs each whose comment count changed. It saves its state and
//! arms the next tick; all of it commits with the call. A fetch that fails is
//! traced and tried again when next due.

#![no_std]

extern crate alloc;

pub mod rss;
mod state;
pub mod time;

pub use state::{Feed, State, Watch};

use alloc::{string::String, vec::Vec};
use berm_lang::{CallError, Result, Schema};

/// Bytes of logs a tick emits before it fetches no more threads. A call holds
/// at most 16 MiB of writes and logs.
const BUDGET: usize = 8 << 20;

/// When a source fetches. Times are in milliseconds.
pub struct Schedule {
    /// Between ticks.
    pub tick: u64,
    /// Before a feed is fetched again.
    pub refetch: u64,
    /// Feeds fetched per tick.
    pub feeds: usize,
    /// Threads fetched per tick.
    pub threads: usize,
    /// Before a watched thread is fetched again.
    pub rewarm: u64,
    /// After an item was posted that its thread stays watched.
    pub window: u64,
}

/// One item a feed showed.
pub struct Listed<T> {
    pub id: String,
    /// When it was posted, in Unix milliseconds.
    pub posted: u64,
    pub item: T,
}

/// A source: how it fetches and what it logs.
pub trait Source {
    /// The name the service is deployed under, which its wake names.
    const NAME: &'static str;
    const SCHEDULE: Schedule;
    type Item;
    type Thread;

    /// The items `feed` shows now.
    fn fetch(feed: &Feed) -> Result<Vec<Listed<Self::Item>>, String>;

    /// Log `item`, new to `feed`. Answers the bytes logged.
    fn post(feed: &Feed, item: Self::Item) -> Result<usize, CallError>;

    /// Whether the threads of the items `feed` shows are fetched.
    fn watches(feed: &Feed) -> bool;

    /// The thread under `watch` and its comment count.
    fn thread(watch: &Watch) -> Result<(u64, Self::Thread), String>;

    /// Log `thread`. Answers the bytes logged.
    fn reply(watch: &Watch, thread: Self::Thread) -> Result<usize, CallError>;
}

/// What [`start`] answers.
#[derive(Schema)]
pub enum Start {
    /// Its tick is armed now.
    Started,
    /// A tick ran within the last three.
    Running,
}

/// Arm `S`'s tick now, unless a tick ran within the last three.
pub fn start<S: Source>() -> Result<Start> {
    let state = State::load()?;
    if berm_lang::now().saturating_sub(state.ticked) < 3 * S::SCHEDULE.tick {
        return Ok(Start::Running);
    }
    berm_lang::after(0, S::NAME, "tick", "")?;
    Ok(Start::Started)
}

/// One tick of `S`.
pub fn tick<S: Source>() -> Result<()> {
    let schedule = S::SCHEDULE;
    let mut state = State::load()?;
    let now = berm_lang::now();
    let mut emitted = 0;
    let State { feeds, watched, .. } = &mut state;

    feeds.sort_by_key(|feed| feed.fetched);
    let due = feeds
        .iter_mut()
        .filter(|feed| feed.fetched == 0 || now.saturating_sub(feed.fetched) >= schedule.refetch)
        .take(schedule.feeds);
    for feed in due {
        feed.fetched = now;
        match S::fetch(feed) {
            Ok(listed) => emitted += posts::<S>(feed, listed, watched)?,
            Err(error) => berm_lang::trace(&error),
        }
    }

    watched.retain(|watch| now.saturating_sub(watch.posted) < schedule.window);
    watched.sort_by_key(|watch| watch.warmed);
    let due = watched
        .iter_mut()
        .filter(|watch| watch.warmed == 0 || now.saturating_sub(watch.warmed) >= schedule.rewarm)
        .take(schedule.threads);
    for watch in due {
        if emitted >= BUDGET {
            break;
        }
        watch.warmed = now;
        match S::thread(watch) {
            Ok((count, _)) if count == watch.comments => {}
            Ok((count, thread)) => {
                watch.comments = count;
                emitted += S::reply(watch, thread)?;
            }
            Err(error) => berm_lang::trace(&error),
        }
    }

    state.ticked = now;
    state.save()?;
    berm_lang::after(schedule.tick, S::NAME, "tick", "")?;
    Ok(())
}

/// Log the items in `listed` new to `feed`, watch them when `S` watches
/// `feed`, and record what it showed. Answers the bytes logged.
fn posts<S: Source>(
    feed: &mut Feed,
    listed: Vec<Listed<S::Item>>,
    watched: &mut Vec<Watch>,
) -> Result<usize, CallError> {
    let watches = S::watches(feed);
    let mut newest = feed.watermark;
    let mut shown = Vec::new();
    let mut emitted = 0;
    for Listed { id, posted, item } in listed {
        newest = newest.max(posted);
        if watches && !watched.iter().any(|watch| watch.id == id) {
            watched.push(Watch {
                id: id.clone(),
                feed: feed.key.clone(),
                posted,
                warmed: 0,
                comments: 0,
            });
        }
        let new = match feed.chronological {
            true => posted > feed.watermark,
            false => {
                let new = !feed.shown.contains(&id);
                shown.push(id);
                new
            }
        };
        if new {
            emitted += S::post(feed, item)?;
        }
    }
    feed.watermark = newest;
    feed.shown = shown;
    Ok(emitted)
}
