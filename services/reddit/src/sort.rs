//! How a subreddit's listing is ranked.

/// A listing's ranking, spelled `new`, `hot`, `rising`, `top-{window}` or
/// `controversial-{window}`, where the window is `hour`, `day`, `week`,
/// `month`, `year` or `all`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    pub segment: &'static str,
    pub window: Option<&'static str>,
}

const WINDOWS: [&str; 6] = ["hour", "day", "week", "month", "year", "all"];

impl Sort {
    pub fn parse(spelled: &str) -> Option<Self> {
        let plain = |segment| {
            Some(Sort {
                segment,
                window: None,
            })
        };
        match spelled {
            "new" => plain("new"),
            "hot" => plain("hot"),
            "rising" => plain("rising"),
            _ => {
                let (kind, window) = spelled.split_once('-')?;
                let segment = match kind {
                    "top" => "top",
                    "controversial" => "controversial",
                    _ => return None,
                };
                let window = WINDOWS.into_iter().find(|w| *w == window)?;
                Some(Sort {
                    segment,
                    window: Some(window),
                })
            }
        }
    }

    /// Whether the listing is ordered by post time, and so resumes from a
    /// watermark. A ranked listing is pulled whole and a post counts as new
    /// when it enters it.
    pub fn chronological(self) -> bool {
        self.segment == "new"
    }
}
