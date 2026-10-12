//! Hacker News through `hn.algolia.com`, keyless. Listings come from
//! `search_by_date`; a story's whole comment tree from `/items/{id}` in one
//! request.

use crate::lane::Lane;
use alloc::{format, string::String, vec::Vec};
use berm_lang::serde::Deserialize;

const API: &str = "https://hn.algolia.com/api/v1";

/// A `search_by_date` page.
#[derive(Deserialize)]
#[serde(crate = "berm_lang::serde")]
pub struct Page {
    pub hits: Vec<Hit>,
}

/// One story from `search_by_date`. `url` is absent on a text post, which
/// carries `story_text` instead.
#[derive(Deserialize)]
#[serde(crate = "berm_lang::serde")]
pub struct Hit {
    #[serde(rename = "objectID")]
    pub object_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub story_text: Option<String>,
    pub created_at_i: u64,
    #[serde(default)]
    pub points: Option<u64>,
    #[serde(default)]
    pub num_comments: Option<u64>,
}

/// One node of the `/items/{id}` tree; the story and its comments share it.
/// `text` is absent on the story and on deleted comments.
#[derive(Deserialize)]
#[serde(crate = "berm_lang::serde")]
pub struct Item {
    pub id: u64,
    pub created_at_i: u64,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub points: Option<u64>,
    #[serde(default)]
    pub children: Vec<Item>,
}

/// The newest page of `lane`.
pub fn search(lane: Lane) -> Result<Page, String> {
    get(&format!(
        "{API}/search_by_date?tags={}&hitsPerPage={}",
        lane.tag(),
        lane.page_size()
    ))
}

/// Story `id` with its comment tree.
pub fn item(id: u64) -> Result<Item, String> {
    get(&format!("{API}/items/{id}"))
}

fn get<T: for<'de> Deserialize<'de>>(url: &str) -> Result<T, String> {
    let response = berm_lang::http::fetch("GET", url, &[], b"")
        .map_err(|error| format!("{url}: {}", error.message()))?;
    if response.status != 200 {
        return Err(format!("{url}: status {}", response.status));
    }
    berm_lang::serde_json::from_slice(&response.body).map_err(|error| format!("{url}: {error}"))
}
