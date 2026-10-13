//! Reddit as a service: the posts of followed subreddit listings and their
//! comments, fetched from Reddit's RSS feeds and emitted as logs.

#![cfg_attr(target_arch = "wasm32", no_std, no_main)]

extern crate alloc;

use alloc::{string::String, vec::Vec};
use berm_lang::{Schema, error_code, event};

mod reddit;
mod sort;
mod source;

#[berm_lang::program(
    syscalls = {
        "berm.get",
        "berm.set",
        "berm.call.after",
        "berm.http.proxy" = ["www.reddit.com"],
    },
)]
mod tools {
    use crate::{Follow, Listing, RedditError, Unfollow, sort::Sort, source::Reddit};
    use alloc::{format, string::String, vec::Vec};
    use berm_lang::Result;
    use lake::Start;

    /// Starts fetching Reddit, unless it already is.
    #[public]
    #[callers(accounts)]
    pub fn start() -> Result<Start> {
        lake::start::<Reddit>()
    }

    /// Follows a subreddit's listing: its posts are logged from the next tick.
    #[public]
    #[callers(services)]
    pub fn follow(
        /// The subreddit's name, without `r/`.
        sub: String,
        /// `new`, `hot`, `rising`, `top-{window}` or `controversial-{window}`,
        /// where the window is `hour`, `day`, `week`, `month`, `year` or
        /// `all`.
        sort: String,
    ) -> Result<Follow> {
        let (key, sort) = listing(sub, sort)?;
        let mut state = lake::State::load()?;
        if !state.follow(&key, sort.chronological()) {
            return Ok(Follow::Following);
        }
        state.save()?;
        Ok(Follow::Followed)
    }

    /// Stops following a subreddit's listing.
    #[public]
    #[callers(services)]
    pub fn unfollow(
        /// The subreddit's name, without `r/`.
        sub: String,
        /// The listing's sort, as `follow` took it.
        sort: String,
    ) -> Result<Unfollow> {
        let (key, _) = listing(sub, sort)?;
        let mut state = lake::State::load()?;
        if !state.unfollow(&key) {
            return Ok(Unfollow::NotFollowing);
        }
        state.save()?;
        Ok(Unfollow::Unfollowed)
    }

    /// Lists the followed listings.
    #[query]
    #[public]
    pub fn follows() -> Result<Vec<Listing>> {
        let state = lake::State::load()?;
        Ok(state
            .feeds
            .iter()
            .filter_map(|feed| {
                let (sub, sort) = feed.key.split_once('/')?;
                Some(Listing {
                    sub: String::from(sub),
                    sort: String::from(sort),
                })
            })
            .collect())
    }

    /// Fetches the listings due and logs each post new to one, fetches the
    /// comments due and logs each post's that changed, then arms the next tick.
    pub fn tick() -> Result<()> {
        lake::tick::<Reddit>()
    }

    /// The feed key of a listing, `{sub}/{sort}` with the subreddit
    /// lowercased, refusing a sort or a subreddit name Reddit does not have.
    fn listing(mut sub: String, sort: String) -> Result<(String, Sort)> {
        sub.make_ascii_lowercase();
        let named = (2..=21).contains(&sub.len())
            && sub.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
        if !named {
            return Err(RedditError::NotSubreddit.into());
        }
        let parsed = Sort::parse(&sort).ok_or(RedditError::NotSort)?;
        Ok((format!("{sub}/{sort}"), parsed))
    }
}

/// What `follow` answers.
#[derive(Schema)]
pub enum Follow {
    Followed,
    /// It was already followed.
    Following,
}

/// What `unfollow` answers.
#[derive(Schema)]
pub enum Unfollow {
    Unfollowed,
    NotFollowing,
}

/// Why a listing is refused.
#[error_code]
pub enum RedditError {
    #[msg("not a subreddit name")]
    NotSubreddit,
    #[msg("not a sort")]
    NotSort,
}

/// A followed subreddit listing.
#[derive(Schema)]
pub struct Listing {
    /// The subreddit's name, without `r/`.
    pub sub: String,
    /// `new`, `hot`, `rising`, `top-{window}` or `controversial-{window}`,
    /// where the window is `hour`, `day`, `week`, `month`, `year` or
    /// `all`.
    pub sort: String,
}

/// A post new to a followed listing: made after the newest post a `new`
/// listing had shown, or newly in a ranked one. A post in several
/// listings is logged once in each.
#[event]
pub struct Post {
    /// Its Reddit id, without `t3_`; its comments are at
    /// `https://www.reddit.com/r/{sub}/comments/{id}/`.
    pub id: String,
    /// The subreddit, lowercased.
    pub sub: String,
    /// The listing's sort.
    pub sort: String,
    pub title: String,
    /// The linked page; absent on a text post.
    pub url: Option<String>,
    /// A text post's body, as plain text.
    pub text: Option<String>,
    pub author: Option<String>,
    /// When it was made, in Unix milliseconds.
    pub posted: u64,
    pub thumbnail: Option<String>,
}

/// A post's comments, logged when the feed holds a different number of
/// them than the last `Comments` of that post. Posts are fetched for 48
/// hours after they were made.
#[event]
pub struct Comments {
    /// The post's id, without `t3_`.
    pub post: String,
    pub sub: String,
    /// Up to 100, in Reddit's top order. Reddit's feed is flat and carries
    /// no scores.
    pub comments: Vec<Comment>,
}

/// One comment in a `Comments`.
#[derive(Schema)]
pub struct Comment {
    /// Its Reddit id, without `t1_`.
    pub id: String,
    pub author: Option<String>,
    /// Plain text.
    pub text: String,
    /// When it was made, in Unix milliseconds.
    pub posted: u64,
}
