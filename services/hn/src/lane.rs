//! The lanes of Hacker News this service follows.

/// One Hacker News lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    FrontPage,
    Newest,
    Ask,
    Show,
}

impl Lane {
    pub const ALL: [Lane; 4] = [Lane::FrontPage, Lane::Newest, Lane::Ask, Lane::Show];

    /// The name a `Story` log carries.
    pub fn name(self) -> &'static str {
        match self {
            Lane::FrontPage => "frontpage",
            Lane::Newest => "newest",
            Lane::Ask => "ask",
            Lane::Show => "show",
        }
    }

    /// The lane `name` names.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|lane| lane.name() == name)
    }

    /// The Algolia `tags` filter selecting this lane.
    pub fn tag(self) -> &'static str {
        match self {
            Lane::FrontPage => "front_page",
            Lane::Newest => "story",
            Lane::Ask => "ask_hn",
            Lane::Show => "show_hn",
        }
    }

    /// Whether the lane is ordered by post time, and so resumes from a
    /// watermark. The front page is a ranking: a story reaches it long after it
    /// was posted, so it is pulled whole and a story counts as new when it
    /// enters it.
    pub fn chronological(self) -> bool {
        !matches!(self, Lane::FrontPage)
    }

    /// Hits per fetch. Algolia's `front_page` tag held 30 stories on
    /// 2026-10-12.
    pub fn page_size(self) -> u32 {
        match self {
            Lane::FrontPage => 200,
            _ => 30,
        }
    }

    /// Whether this lane's stories have their discussions fetched.
    pub fn warmed(self) -> bool {
        !matches!(self, Lane::Newest)
    }

    pub fn url(self) -> &'static str {
        match self {
            Lane::FrontPage => "https://news.ycombinator.com",
            Lane::Newest => "https://news.ycombinator.com/newest",
            Lane::Ask => "https://news.ycombinator.com/ask",
            Lane::Show => "https://news.ycombinator.com/show",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Lane::FrontPage => "Hacker News · frontpage",
            Lane::Newest => "Hacker News · newest",
            Lane::Ask => "Hacker News · ask",
            Lane::Show => "Hacker News · show",
        }
    }

    /// What a reader following this lane gets.
    pub fn description(self) -> &'static str {
        match self {
            Lane::FrontPage => {
                "What the community voted up today — startups, software, and \
                 the science and business around them."
            }
            Lane::Newest => {
                "Every submission as it arrives, unranked and unfiltered — the \
                 firehose before the votes land."
            }
            Lane::Ask => {
                "Ask HN: questions put to the community, and the argument they \
                 draw out."
            }
            Lane::Show => {
                "Show HN: things people built and shipped, posted by the people \
                 who built them."
            }
        }
    }
}
