//! Hacker News as a service: stories and their discussions, fetched from
//! Algolia's Hacker News API and emitted as logs.

#![cfg_attr(target_arch = "wasm32", no_std, no_main)]

extern crate alloc;

mod algolia;
mod lane;
mod source;

#[berm_lang::program(
    syscalls = {
        "berm.get",
        "berm.set",
        "berm.call.after",
        "berm.http.fetch" = ["hn.algolia.com"],
    },
)]
mod tools {
    use crate::{lane::Lane, source::Hn};
    use alloc::{string::String, vec::Vec};
    use berm_lang::{Failed, Out};

    /// Starts fetching Hacker News, unless it already is. Answers `started` or
    /// `running`.
    #[public]
    #[callers(accounts)]
    pub fn start(_args: &[u8], out: &mut Out) -> Result<(), Failed> {
        let mut state = berm_lang::tool::syscall(lake::State::load(), out)?;
        let mut followed = false;
        for lane in Lane::ALL {
            followed |= state.follow(lane.name(), lane.chronological());
        }
        if followed {
            berm_lang::tool::syscall(state.save(), out)?;
        }
        lake::start::<Hn>(out)
    }

    /// Fetches the lanes and logs each story new to one, fetches the
    /// discussions due and logs each that changed, then arms the next tick.
    pub fn tick(_args: &[u8], out: &mut Out) -> Result<(), Failed> {
        lake::tick::<Hn>(out)
    }

    /// Lists the Hacker News lanes this service follows.
    #[query]
    #[public]
    #[returns(Lanes)]
    pub fn lanes(_args: &[u8], out: &mut Out) -> Result<(), Failed> {
        let lanes = Lane::ALL
            .iter()
            .map(|lane| LaneInfo {
                name: lane.name(),
                label: lane.label(),
                url: lane.url(),
                description: lane.description(),
            })
            .collect();
        berm_lang::tool::reply(&Lanes { lanes }, out)
    }

    /// What `lanes` answers.
    pub struct Lanes {
        pub lanes: Vec<LaneInfo>,
    }

    /// One lane.
    pub struct LaneInfo {
        /// What a `Story` log names it.
        pub name: &'static str,
        pub label: &'static str,
        /// The lane on news.ycombinator.com.
        pub url: &'static str,
        /// What a reader following it gets.
        pub description: &'static str,
    }

    /// A story new to a lane: posted after the newest story the lane had shown,
    /// or newly on the front page. A story in several lanes is logged once in
    /// each.
    #[event]
    pub struct Story {
        /// Its Hacker News item id; its thread is
        /// `https://news.ycombinator.com/item?id={id}`.
        pub id: u64,
        /// `frontpage`, `newest`, `ask` or `show`.
        pub lane: String,
        pub title: String,
        /// The linked article; absent on a text post.
        pub url: Option<String>,
        /// A text post's body, as plain text.
        pub text: Option<String>,
        pub author: Option<String>,
        /// When it was posted, in Unix milliseconds.
        pub posted: u64,
        pub points: Option<u64>,
        /// Its comment count when logged.
        pub comments: Option<u64>,
    }

    /// A story's discussion, logged when it holds a different number of
    /// comments than the last `Thread` of that story. Stories on the front
    /// page, Ask HN and Show HN are fetched for 48 hours after posting.
    #[event]
    pub struct Thread {
        /// The story's item id.
        pub story: u64,
        /// The story's points when fetched.
        pub points: Option<u64>,
        /// The first 100 top-level comments and their replies to six levels,
        /// depth-first, each after its parent.
        pub comments: Vec<Comment>,
    }

    /// One comment in a `Thread`.
    pub struct Comment {
        pub id: u64,
        /// The comment it replies to, or the story.
        pub parent: u64,
        pub author: Option<String>,
        /// Plain text; empty for a deleted comment with replies.
        pub text: String,
        /// When it was posted, in Unix milliseconds.
        pub posted: u64,
    }
}
