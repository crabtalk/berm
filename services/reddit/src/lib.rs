//! Reddit as a service: the posts of followed subreddit listings and their
//! comments, fetched from Reddit's RSS feeds and emitted as logs.

#![cfg_attr(target_arch = "wasm32", no_std, no_main)]

extern crate alloc;

mod atom;
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
    use crate::{sort::Sort, source::Reddit};
    use alloc::{format, string::String, vec::Vec};
    use berm_lang::{Failed, Out};

    /// Starts fetching Reddit, unless it already is. Answers `started` or
    /// `running`.
    #[public]
    #[callers(accounts)]
    pub fn start(_args: &[u8], out: &mut Out) -> Result<(), Failed> {
        lake::start::<Reddit>(out)
    }

    /// Follows a subreddit's listing: its posts are logged from the next tick.
    /// Answers `followed` or `following`.
    #[public]
    #[callers(services)]
    #[args(Listing)]
    pub fn follow(args: &[u8], out: &mut Out) -> Result<(), Failed> {
        let (key, sort) = listing(args, out)?;
        let mut state = berm_lang::tool::syscall(lake::State::load(), out)?;
        if !state.follow(&key, sort.chronological()) {
            out.write(b"following");
            return Ok(());
        }
        berm_lang::tool::syscall(state.save(), out)?;
        out.write(b"followed");
        Ok(())
    }

    /// Stops following a subreddit's listing. Answers `unfollowed` or
    /// `not following`.
    #[public]
    #[callers(services)]
    #[args(Listing)]
    pub fn unfollow(args: &[u8], out: &mut Out) -> Result<(), Failed> {
        let (key, _) = listing(args, out)?;
        let mut state = berm_lang::tool::syscall(lake::State::load(), out)?;
        if !state.unfollow(&key) {
            out.write(b"not following");
            return Ok(());
        }
        berm_lang::tool::syscall(state.save(), out)?;
        out.write(b"unfollowed");
        Ok(())
    }

    /// Lists the followed listings.
    #[query]
    #[public]
    #[returns(Follows)]
    pub fn follows(_args: &[u8], out: &mut Out) -> Result<(), Failed> {
        let state = berm_lang::tool::syscall(lake::State::load(), out)?;
        let follows = state
            .feeds
            .iter()
            .filter_map(|feed| {
                let (sub, sort) = feed.key.split_once('/')?;
                Some(Listing {
                    sub: String::from(sub),
                    sort: String::from(sort),
                })
            })
            .collect();
        berm_lang::tool::reply(&Follows { follows }, out)
    }

    /// Fetches the listings due and logs each post new to one, fetches the
    /// comments due and logs each post's that changed, then arms the next tick.
    pub fn tick(_args: &[u8], out: &mut Out) -> Result<(), Failed> {
        lake::tick::<Reddit>(out)
    }

    /// A subreddit's listing.
    pub struct Listing {
        /// The subreddit's name, without `r/`.
        pub sub: String,
        /// `new`, `hot`, `rising`, `top-{window}` or `controversial-{window}`,
        /// where the window is `hour`, `day`, `week`, `month`, `year` or
        /// `all`.
        pub sort: String,
    }

    /// What `follows` answers.
    pub struct Follows {
        pub follows: Vec<Listing>,
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
    pub struct Comment {
        /// Its Reddit id, without `t1_`.
        pub id: String,
        pub author: Option<String>,
        /// Plain text.
        pub text: String,
        /// When it was made, in Unix milliseconds.
        pub posted: u64,
    }

    /// The feed key of the listing in `args`, `{sub}/{sort}` with the
    /// subreddit lowercased, refusing a sort or a subreddit name Reddit does
    /// not have.
    fn listing(args: &[u8], out: &mut Out) -> Result<(String, Sort), Failed> {
        let Listing { mut sub, sort } = berm_lang::tool::parse(args, out)?;
        sub.make_ascii_lowercase();
        let named = (2..=21).contains(&sub.len())
            && sub.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
        if !named {
            berm_lang::tool::failed("not a subreddit name", out)?;
        }
        let Some(parsed) = Sort::parse(&sort) else {
            berm_lang::tool::failed("not a sort", out)?;
            return Err(Failed);
        };
        Ok((format!("{sub}/{sort}"), parsed))
    }
}
