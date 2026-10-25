//! What providers return, independent of any one provider's API.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SearchResult {
    pub provider: String,
    pub id: String,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i64>,
    pub overview: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeriesStatus {
    Continuing,
    Ended,
    Cancelled,
}

impl SeriesStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SeriesStatus::Continuing => "continuing",
            SeriesStatus::Ended => "ended",
            SeriesStatus::Cancelled => "cancelled",
        }
    }
}

/// When an episode airs. Providers that only know the date give `time_known: false`;
/// `at` is then the end of that day in UTC, so nothing counts as aired too early.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AirTime {
    /// Unix seconds, UTC.
    pub at: i64,
    pub time_known: bool,
}

impl AirTime {
    /// From a `YYYY-MM-DD` date with no time of day.
    pub fn from_date(date: &str) -> Option<AirTime> {
        let day = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
        let end = day.succ_opt()?.and_hms_opt(0, 0, 0)?.and_utc().timestamp();
        Some(AirTime {
            at: end,
            time_known: false,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpisodeData {
    /// The provider's episode ID: the episode's identity.
    pub id: String,
    pub title: String,
    pub overview: Option<String>,
    pub airs: Option<AirTime>,
    /// The network's IANA time zone, when the provider knows the time of day.
    pub airs_tz: Option<String>,
    pub runtime_minutes: Option<i64>,
    /// Aired order. Season 0 holds specials.
    pub season: i64,
    pub number: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderingEntryData {
    pub episode_id: String,
    /// None for flat orderings such as absolute.
    pub season: Option<i64>,
    pub number: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderingData {
    pub name: String,
    /// `dvd`, `absolute`, `story` or `custom`. Aired order comes from the episodes.
    pub kind: String,
    pub entries: Vec<OrderingEntryData>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeasonData {
    pub number: i64,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub poster: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriesData {
    pub provider: String,
    pub id: String,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i64>,
    pub overview: Option<String>,
    pub status: Option<SeriesStatus>,
    pub genres: Vec<String>,
    /// Other providers' IDs for the series: (provider, id).
    pub external_ids: Vec<(String, String)>,
    pub seasons: Vec<SeasonData>,
    pub episodes: Vec<EpisodeData>,
    /// Orderings besides aired order.
    pub orderings: Vec<OrderingData>,
    pub poster: Option<String>,
    pub backdrop: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MovieData {
    pub provider: String,
    pub id: String,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i64>,
    pub overview: Option<String>,
    pub runtime_minutes: Option<i64>,
    pub release: Option<AirTime>,
    pub genres: Vec<String>,
    pub external_ids: Vec<(String, String)>,
    pub poster: Option<String>,
    pub backdrop: Option<String>,
}
