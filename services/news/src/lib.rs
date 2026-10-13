//! News as a service: the articles of curated news desks, fetched from their
//! RSS and Atom feeds, or from an index page where a desk has none, and
//! emitted as logs.

#![cfg_attr(target_arch = "wasm32", no_std, no_main)]

extern crate alloc;

use alloc::{string::String, vec::Vec};
use berm_lang::{Schema, event};

mod catalog;
mod page;
mod source;
mod topic;
mod web;

#[berm_lang::program(
    buffer = 65536,
    syscalls = {
        "berm.get",
        "berm.set",
        "berm.call.after",
        "berm.http.fetch" = [
            "abovethelaw.com",
            "api.axios.com",
            "api.theregister.com",
            "blog.google",
            "deepmind.google",
            "digiday.com",
            "electrek.co",
            "feeds.arstechnica.com",
            "feeds.bbci.co.uk",
            "feeds.npr.org",
            "fortune.com",
            "frontofficesports.com",
            "insideevs.com",
            "openai.com",
            "phys.org",
            "pitchfork.com",
            "research.google",
            "restofworld.org",
            "rss.nytimes.com",
            "search.cnbc.com",
            "skift.com",
            "spectrum.ieee.org",
            "techcrunch.com",
            "theconversation.com",
            "toybook.com",
            "variety.com",
            "www.404media.co",
            "www.aljazeera.com",
            "www.anthropic.com",
            "www.bevnet.com",
            "www.bleepingcomputer.com",
            "www.bloomberg.com",
            "www.canarymedia.com",
            "www.carbonbrief.org",
            "www.cbc.ca",
            "www.coindesk.com",
            "www.creativebloq.com",
            "www.dezeen.com",
            "www.eater.com",
            "www.economist.com",
            "www.eurogamer.net",
            "www.fastcompany.com",
            "www.fooddive.com",
            "www.ft.com",
            "www.gamedeveloper.com",
            "www.glossy.co",
            "www.grocerydive.com",
            "www.insidehighered.com",
            "www.marketingdive.com",
            "www.modernretail.co",
            "www.musicbusinessworldwide.com",
            "www.nature.com",
            "www.nosh.com",
            "www.packagingdive.com",
            "www.phoronix.com",
            "www.polygon.com",
            "www.practicalecommerce.com",
            "www.quantamagazine.org",
            "www.restaurantdive.com",
            "www.retaildive.com",
            "www.science.org",
            "www.scmp.com",
            "www.sportico.com",
            "www.statnews.com",
            "www.supplychaindive.com",
            "www.technologyreview.com",
            "www.theblock.co",
            "www.theguardian.com",
            "www.theverge.com",
            "www.tomshardware.com",
            "www.wired.com",
        ],
    },
)]
mod tools {
    use crate::{
        DeskInfo, Desks, TopicInfo, Topics,
        catalog::{DESKS, Desk, Reader},
        source::News,
        topic,
    };
    use berm_lang::Result;
    use lake::Start;

    /// Starts fetching every desk, unless it already is.
    #[public]
    #[callers(accounts)]
    pub fn start() -> Result<Start> {
        let mut state = lake::State::load()?;
        let before = state.feeds.len();
        state.feeds.retain(|feed| Desk::find(&feed.key).is_some());
        let mut changed = state.feeds.len() != before;
        for desk in DESKS {
            changed |= state.follow(desk.key, matches!(desk.reader, Reader::Feed(_)));
        }
        if changed {
            state.save()?;
        }
        lake::start::<News>()
    }

    /// Fetches the desks due and logs each article new to one, then arms the
    /// next tick.
    pub fn tick() -> Result<()> {
        lake::tick::<News>()
    }

    /// Lists the desks this service follows.
    #[query]
    #[public]
    pub fn desks() -> Result<Desks> {
        let desks = DESKS
            .iter()
            .map(|desk| DeskInfo {
                key: desk.key.into(),
                outlet: desk.outlet.into(),
                name: desk.name.into(),
                url: desk.home.into(),
                description: desk.description.into(),
                topics: desk.topics.iter().map(|topic| topic.name.into()).collect(),
            })
            .collect();
        Ok(Desks { desks })
    }

    /// Lists the topics desks are tagged with.
    #[query]
    #[public]
    pub fn topics() -> Result<Topics> {
        let topics = topic::ALL
            .iter()
            .map(|topic| TopicInfo {
                name: topic.name.into(),
                label: topic.label.into(),
                description: topic.description.into(),
            })
            .collect();
        Ok(Topics { topics })
    }
}

/// What `desks` answers.
#[derive(Schema)]
pub struct Desks {
    pub desks: Vec<DeskInfo>,
}

/// One desk.
#[derive(Schema)]
pub struct DeskInfo {
    /// `{outlet}` or `{outlet}/{section}`; what an `Article` names it.
    pub key: String,
    pub outlet: String,
    /// Its name within its outlet.
    pub name: String,
    /// The page a person reads it on.
    pub url: String,
    /// What a reader following it gets.
    pub description: String,
    /// The names of the topics it covers.
    pub topics: Vec<String>,
}

/// What `topics` answers.
#[derive(Schema)]
pub struct Topics {
    pub topics: Vec<TopicInfo>,
}

/// One topic.
#[derive(Schema)]
pub struct TopicInfo {
    /// What `DeskInfo::topics` names it.
    pub name: String,
    pub label: String,
    pub description: String,
}

/// An article new to a desk: published after the newest article its feed
/// had shown, or newly on its index page. An article on several desks is
/// logged once in each.
#[event]
pub struct Article {
    /// The desk's key.
    pub desk: String,
    pub title: String,
    pub url: String,
    /// The feed's summary of it, as plain text.
    pub summary: Option<String>,
    /// The feed's full text of it, as plain text.
    pub content: Option<String>,
    pub author: Option<String>,
    /// When it was published, in Unix milliseconds. An index page dates an
    /// article to midnight UTC of its day.
    pub posted: u64,
    pub image: Option<String>,
}
