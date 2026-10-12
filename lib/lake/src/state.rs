//! What a source keeps between ticks, under one `berm.set` key.

use alloc::{string::String, vec::Vec};
use berm_lang::{
    CallError,
    serde::{Deserialize, Serialize},
};

const KEY: &str = "lake";

#[derive(Default, Serialize, Deserialize)]
#[serde(crate = "berm_lang::serde")]
pub struct State {
    /// When the last tick ran, in Unix milliseconds.
    pub ticked: u64,
    pub feeds: Vec<Feed>,
    /// The items whose threads are fetched.
    pub watched: Vec<Watch>,
}

/// A followed feed.
#[derive(Serialize, Deserialize)]
#[serde(crate = "berm_lang::serde")]
pub struct Feed {
    /// What the source names it by.
    pub key: String,
    /// Ordered by post time: an item is new past `watermark`. Otherwise an
    /// item is new when it was not in `shown`.
    pub chronological: bool,
    /// When it was last fetched, in Unix milliseconds; 0 if never.
    pub fetched: u64,
    /// The newest post time it has shown, in Unix milliseconds.
    pub watermark: u64,
    /// The items it showed when last fetched.
    pub shown: Vec<String>,
}

/// An item whose thread is fetched.
#[derive(Serialize, Deserialize)]
#[serde(crate = "berm_lang::serde")]
pub struct Watch {
    pub id: String,
    /// The key of the feed that first showed it.
    pub feed: String,
    /// When it was posted, in Unix milliseconds.
    pub posted: u64,
    /// When its thread was last fetched, in Unix milliseconds; 0 if never.
    pub warmed: u64,
    /// The comment count its thread was last logged with.
    pub comments: u64,
}

impl State {
    pub fn load() -> Result<Self, CallError> {
        Ok(berm_lang::get_json(KEY)?.unwrap_or_default())
    }

    pub fn save(&self) -> Result<(), CallError> {
        berm_lang::set_json(KEY, self)
    }

    /// Follow the feed `key`, unless it already is. Answers whether it was
    /// added.
    pub fn follow(&mut self, key: &str, chronological: bool) -> bool {
        if self.feeds.iter().any(|feed| feed.key == key) {
            return false;
        }
        self.feeds.push(Feed {
            key: String::from(key),
            chronological,
            fetched: 0,
            watermark: 0,
            shown: Vec::new(),
        });
        true
    }

    /// Stop following the feed `key`. Answers whether it was followed.
    pub fn unfollow(&mut self, key: &str) -> bool {
        let before = self.feeds.len();
        self.feeds.retain(|feed| feed.key != key);
        self.feeds.len() != before
    }
}
