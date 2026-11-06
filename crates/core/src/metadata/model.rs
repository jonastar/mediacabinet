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
    /// Guest stars and crew; the series' regulars are on the series.
    #[serde(default)]
    pub credits: Credits,
    #[serde(default)]
    pub rating: Option<RatingData>,
    /// A frame from the episode, as a provider image path.
    #[serde(default)]
    pub still: Option<String>,
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
    #[serde(default)]
    pub tagline: Option<String>,
    #[serde(default)]
    pub content_rating: Option<String>,
    /// Regular cast, and creators as crew.
    #[serde(default)]
    pub credits: Credits,
    #[serde(default)]
    pub networks: Vec<CompanyData>,
    #[serde(default)]
    pub studios: Vec<CompanyData>,
    #[serde(default)]
    pub rating: Option<RatingData>,
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
    #[serde(default)]
    pub tagline: Option<String>,
    #[serde(default)]
    pub content_rating: Option<String>,
    #[serde(default)]
    pub credits: Credits,
    #[serde(default)]
    pub studios: Vec<CompanyData>,
    #[serde(default)]
    pub rating: Option<RatingData>,
}

/// Someone as a provider knows them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonRef {
    pub id: String,
    pub name: String,
    /// The provider's image path.
    #[serde(default)]
    pub photo: Option<String>,
    /// Acting, Directing, Writing...
    #[serde(default)]
    pub known_for: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreditData {
    pub person: PersonRef,
    /// The character for cast and guests, the job for crew.
    #[serde(default)]
    pub role: Option<String>,
    /// Crew only.
    #[serde(default)]
    pub department: Option<String>,
    /// A series' cast: how many episodes they're in.
    #[serde(default)]
    pub episodes: Option<i64>,
}

/// In billing order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credits {
    #[serde(default)]
    pub cast: Vec<CreditData>,
    #[serde(default)]
    pub guests: Vec<CreditData>,
    #[serde(default)]
    pub crew: Vec<CreditData>,
}

/// A network or studio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanyData {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RatingData {
    pub value: f64,
    /// What a perfect score is: 10 or 100.
    pub scale: f64,
    #[serde(default)]
    pub votes: Option<i64>,
}

/// What a provider knows about a person beyond their credits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonData {
    pub id: String,
    pub name: String,
    pub photo: Option<String>,
    pub known_for: Option<String>,
    pub biography: Option<String>,
    /// `YYYY-MM-DD`.
    pub birthday: Option<String>,
    pub deathday: Option<String>,
    pub birthplace: Option<String>,
}
