//! TMDB (themoviedb.org) API v3.
//!
//! Accepts either a v3 API key or a v4 read access token. Episode groups become
//! orderings: TMDB's group types 2 (absolute), 3 (DVD) and 5 (story arc) map to
//! ours; the rest become custom orderings.
//!
//! Credits come with the same requests: a series' cast from its aggregate
//! credits, each episode's guest stars and crew from its season.

use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::MetadataError;
use super::model::*;

const BASE: &str = "https://api.themoviedb.org/3";
pub const PROVIDER: &str = "tmdb";

/// Cast members kept per series or movie, in billing order.
const CAST_LIMIT: usize = 40;

#[derive(Clone)]
pub struct Tmdb {
    http: reqwest::Client,
    key: String,
    base: String,
    /// ISO 3166-1 country whose age ratings to use.
    region: String,
}

impl Tmdb {
    pub fn new(key: impl Into<String>) -> Tmdb {
        Tmdb::with_base(key, BASE)
    }

    /// Against another base URL, for tests.
    pub fn with_base(key: impl Into<String>, base: impl Into<String>) -> Tmdb {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("MediaCabinet/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP client builds");
        Tmdb {
            http,
            key: key.into(),
            base: base.into(),
            region: "US".into(),
        }
    }

    /// Age ratings from this country (e.g. `DE`) instead of the US.
    pub fn with_region(mut self, region: &str) -> Tmdb {
        self.region = region.to_ascii_uppercase();
        self
    }

    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, MetadataError> {
        let mut request = self.http.get(format!("{}{path}", self.base)).query(query);
        // v4 read access tokens are JWTs; v3 keys are short hex strings.
        request = if self.key.starts_with("eyJ") {
            request.bearer_auth(&self.key)
        } else {
            request.query(&[("api_key", &self.key)])
        };
        let response = request
            .send()
            .await
            .map_err(|e| MetadataError::Unreachable(e.without_url().to_string()))?;
        match response.status().as_u16() {
            200 => response
                .json()
                .await
                .map_err(|e| MetadataError::Invalid(e.without_url().to_string())),
            401 => Err(MetadataError::Auth("TMDB rejected the API key".into())),
            404 => Err(MetadataError::NotFound),
            429 => Err(MetadataError::Unreachable(
                "TMDB rate limit; try again shortly".into(),
            )),
            status => Err(MetadataError::Unreachable(format!(
                "TMDB answered {status}"
            ))),
        }
    }

    pub async fn search_series(
        &self,
        query: &str,
        year: Option<i64>,
    ) -> Result<Vec<SearchResult>, MetadataError> {
        let mut params = vec![("query", query.to_string())];
        if let Some(y) = year {
            params.push(("first_air_date_year", y.to_string()));
        }
        let page: Page<RawTvResult> = self.get("/search/tv", &params).await?;
        Ok(page.results.into_iter().map(Into::into).collect())
    }

    pub async fn search_movies(
        &self,
        query: &str,
        year: Option<i64>,
    ) -> Result<Vec<SearchResult>, MetadataError> {
        let mut params = vec![("query", query.to_string())];
        if let Some(y) = year {
            params.push(("year", y.to_string()));
        }
        let page: Page<RawMovieResult> = self.get("/search/movie", &params).await?;
        Ok(page.results.into_iter().map(Into::into).collect())
    }

    /// Looks up a TMDB ID from another provider's: `tvdb` or `imdb`.
    pub async fn find(
        &self,
        provider: &str,
        id: &str,
    ) -> Result<Option<(String, String)>, MetadataError> {
        let source = match provider {
            "tvdb" => "tvdb_id",
            "imdb" => "imdb_id",
            _ => return Ok(None),
        };
        let found: RawFind = self
            .get(
                &format!("/find/{id}"),
                &[("external_source", source.to_string())],
            )
            .await?;
        Ok(found
            .tv_results
            .first()
            .map(|r| ("series".to_string(), r.id.to_string()))
            .or_else(|| {
                found
                    .movie_results
                    .first()
                    .map(|r| ("movie".to_string(), r.id.to_string()))
            }))
    }

    pub async fn series(&self, id: &str) -> Result<SeriesData, MetadataError> {
        let show: RawShow = self
            .get(
                &format!("/tv/{id}"),
                &[(
                    "append_to_response",
                    "external_ids,episode_groups,aggregate_credits,content_ratings".to_string(),
                )],
            )
            .await?;
        let mut seasons = Vec::new();
        for s in &show.seasons {
            let season: RawSeason = self
                .get(&format!("/tv/{id}/season/{}", s.season_number), &[])
                .await?;
            seasons.push(season);
        }
        let mut groups = Vec::new();
        for g in show
            .episode_groups
            .as_ref()
            .map(|g| g.results.as_slice())
            .unwrap_or_default()
        {
            if let Some(kind) = group_kind(g.kind) {
                let group: RawGroup = self
                    .get(&format!("/tv/episode_group/{}", g.id), &[])
                    .await?;
                groups.push((kind, group));
            }
        }
        Ok(series_from(show, seasons, groups, &self.region))
    }

    pub async fn movie(&self, id: &str) -> Result<MovieData, MetadataError> {
        let movie: RawMovie = self
            .get(
                &format!("/movie/{id}"),
                &[(
                    "append_to_response",
                    "external_ids,credits,release_dates".to_string(),
                )],
            )
            .await?;
        Ok(movie_from(movie, &self.region))
    }

    pub async fn person(&self, id: &str) -> Result<PersonData, MetadataError> {
        let p: RawPersonDetail = self.get(&format!("/person/{id}"), &[]).await?;
        Ok(PersonData {
            id: p.id.to_string(),
            name: p.name,
            photo: p.profile_path,
            known_for: non_empty(p.known_for_department),
            biography: non_empty(p.biography),
            birthday: non_empty(p.birthday),
            deathday: non_empty(p.deathday),
            birthplace: non_empty(p.place_of_birth),
        })
    }
}

fn year(date: &Option<String>) -> Option<i64> {
    date.as_deref()
        .and_then(|d| d.get(..4))
        .and_then(|y| y.parse().ok())
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.trim().is_empty())
}

fn group_kind(kind: i64) -> Option<&'static str> {
    match kind {
        2 => Some("absolute"),
        3 => Some("dvd"),
        5 => Some("story"),
        // Original air date (1) duplicates aired order; 4 digital, 6 production and 7 TV are rarely useful.
        4 | 6 | 7 => Some("custom"),
        _ => None,
    }
}

/// A score, unless nobody has voted yet.
fn rating(average: Option<f64>, votes: Option<i64>) -> Option<RatingData> {
    match (average, votes) {
        (Some(value), Some(votes)) if votes > 0 && value > 0.0 => Some(RatingData {
            value,
            scale: 10.0,
            votes: Some(votes),
        }),
        _ => None,
    }
}

fn person(p: &RawPerson) -> PersonRef {
    PersonRef {
        id: p.id.to_string(),
        name: p.name.clone(),
        photo: p.profile_path.clone(),
        known_for: non_empty(p.known_for_department.clone()),
    }
}

fn companies(list: &[RawCompany]) -> Vec<CompanyData> {
    list.iter()
        .map(|c| CompanyData {
            id: Some(c.id.to_string()),
            name: c.name.clone(),
        })
        .collect()
}

/// Crew worth showing, most prominent first; the rest (grips, caterers) is left out.
const CREW_JOBS: &[&str] = &[
    "Director",
    "Creator",
    "Screenplay",
    "Writer",
    "Teleplay",
    "Story",
    "Novel",
    "Original Story",
    "Characters",
    "Comic Book",
    "Producer",
    "Original Music Composer",
    "Music",
    "Director of Photography",
    "Editor",
];

/// Notable crew, ordered by job then billing; a person keeps one row per job.
fn crew(list: &[RawCrew]) -> Vec<CreditData> {
    let mut picked: Vec<(usize, &RawCrew)> = list
        .iter()
        .filter_map(|c| {
            let rank = CREW_JOBS
                .iter()
                .position(|j| Some(*j) == c.job.as_deref())?;
            Some((rank, c))
        })
        .collect();
    // Stable, so people keep the provider's order within a job.
    picked.sort_by_key(|(rank, _)| *rank);
    let mut seen = std::collections::HashSet::new();
    picked
        .into_iter()
        .filter(|(_, c)| seen.insert((c.person.id, c.job.clone())))
        .map(|(_, c)| CreditData {
            person: person(&c.person),
            role: c.job.clone(),
            department: non_empty(c.department.clone()),
            episodes: None,
        })
        .collect()
}

/// Cast and guest stars in billing order, each person once.
fn cast(list: &[RawCast]) -> Vec<CreditData> {
    let mut sorted: Vec<&RawCast> = list.iter().collect();
    sorted.sort_by_key(|c| c.order.unwrap_or(i64::MAX));
    let mut seen = std::collections::HashSet::new();
    sorted
        .into_iter()
        .filter(|c| seen.insert(c.person.id))
        .take(CAST_LIMIT)
        .map(|c| CreditData {
            person: person(&c.person),
            role: non_empty(c.character.clone()),
            department: None,
            episodes: None,
        })
        .collect()
}

/// A series' cast across all seasons: characters joined, with episode counts.
fn aggregate_cast(list: &[RawAggregateCast]) -> Vec<CreditData> {
    let mut sorted: Vec<&RawAggregateCast> = list.iter().collect();
    sorted.sort_by_key(|c| c.order.unwrap_or(i64::MAX));
    sorted
        .into_iter()
        .take(CAST_LIMIT)
        .map(|c| {
            let characters: Vec<&str> = c
                .roles
                .iter()
                .filter_map(|r| r.character.as_deref())
                .filter(|r| !r.trim().is_empty())
                .collect();
            CreditData {
                person: person(&c.person),
                role: (!characters.is_empty()).then(|| characters.join(" / ")),
                department: None,
                episodes: c.total_episode_count.filter(|n| *n > 0),
            }
        })
        .collect()
}

/// The region's rating, else the US one.
fn pick_rating<'a>(
    region: &str,
    ratings: impl Iterator<Item = (&'a str, &'a str)> + Clone,
) -> Option<String> {
    let find = |want: &str| {
        ratings
            .clone()
            .find(|(country, rating)| *country == want && !rating.trim().is_empty())
            .map(|(_, rating)| rating.trim().to_string())
    };
    find(region).or_else(|| find("US"))
}

pub(crate) fn series_from(
    show: RawShow,
    seasons: Vec<RawSeason>,
    groups: Vec<(&str, RawGroup)>,
    region: &str,
) -> SeriesData {
    let mut external_ids = Vec::new();
    if let Some(ids) = &show.external_ids {
        if let Some(tvdb) = ids.tvdb_id {
            external_ids.push(("tvdb".to_string(), tvdb.to_string()));
        }
        if let Some(imdb) = ids.imdb_id.clone().filter(|s| !s.is_empty()) {
            external_ids.push(("imdb".to_string(), imdb));
        }
    }
    let status = match show.status.as_deref() {
        Some("Ended") => Some(SeriesStatus::Ended),
        Some("Canceled") => Some(SeriesStatus::Cancelled),
        Some(_) => Some(SeriesStatus::Continuing),
        None => None,
    };
    let default_runtime = show.episode_run_time.first().copied();

    let mut episodes = Vec::new();
    let mut season_data = Vec::new();
    for season in seasons {
        season_data.push(SeasonData {
            number: season.season_number,
            title: non_empty(season.name),
            overview: non_empty(season.overview),
            poster: season.poster_path,
        });
        for e in season.episodes {
            episodes.push(EpisodeData {
                id: e.id.to_string(),
                title: non_empty(e.name).unwrap_or_else(|| format!("Episode {}", e.episode_number)),
                overview: non_empty(e.overview),
                airs: e.air_date.as_deref().and_then(AirTime::from_date),
                airs_tz: None,
                runtime_minutes: e.runtime.or(default_runtime),
                season: e.season_number,
                number: e.episode_number,
                credits: Credits {
                    cast: Vec::new(),
                    guests: cast(&e.guest_stars),
                    crew: crew(&e.crew),
                },
                rating: rating(e.vote_average, e.vote_count),
            });
        }
    }

    let mut orderings = Vec::new();
    for (kind, group) in groups {
        let mut entries = Vec::new();
        let flat = kind == "absolute" || group.groups.len() == 1;
        let mut running = 0;
        for (gi, g) in group.groups.iter().enumerate() {
            let mut eps: Vec<&RawGroupEpisode> = g.episodes.iter().collect();
            eps.sort_by_key(|e| e.order);
            for e in eps {
                running += 1;
                entries.push(OrderingEntryData {
                    episode_id: e.id.to_string(),
                    season: if flat {
                        None
                    } else {
                        Some(g.order.unwrap_or(gi as i64 + 1))
                    },
                    number: if flat {
                        running
                    } else {
                        e.order + 1
                    },
                });
            }
        }
        orderings.push(OrderingData {
            name: group.name,
            kind: kind.to_string(),
            entries,
        });
    }

    let content_rating = show.content_ratings.as_ref().and_then(|r| {
        pick_rating(
            region,
            r.results
                .iter()
                .map(|x| (x.iso_3166_1.as_str(), x.rating.as_str())),
        )
    });
    let mut series_crew: Vec<CreditData> = show
        .created_by
        .iter()
        .map(|c| CreditData {
            person: person(c),
            role: Some("Creator".into()),
            department: Some("Writing".into()),
            episodes: None,
        })
        .collect();
    series_crew.dedup_by(|a, b| a.person.id == b.person.id);
    let series_cast = show
        .aggregate_credits
        .as_ref()
        .map(|c| aggregate_cast(&c.cast))
        .unwrap_or_default();

    SeriesData {
        provider: PROVIDER.into(),
        id: show.id.to_string(),
        title: show.name,
        original_title: non_empty(show.original_name),
        year: year(&show.first_air_date),
        overview: non_empty(show.overview),
        status,
        genres: show.genres.into_iter().map(|g| g.name).collect(),
        external_ids,
        seasons: season_data,
        episodes,
        orderings,
        poster: show.poster_path,
        backdrop: show.backdrop_path,
        tagline: non_empty(show.tagline),
        content_rating,
        credits: Credits {
            cast: series_cast,
            guests: Vec::new(),
            crew: series_crew,
        },
        networks: companies(&show.networks),
        studios: companies(&show.production_companies),
        rating: rating(show.vote_average, show.vote_count),
    }
}

pub(crate) fn movie_from(m: RawMovie, region: &str) -> MovieData {
    let mut external_ids = Vec::new();
    let imdb = m.imdb_id.or(m.external_ids.and_then(|e| e.imdb_id));
    if let Some(imdb) = imdb.filter(|s| !s.is_empty()) {
        external_ids.push(("imdb".to_string(), imdb));
    }
    // Theatrical (3) and digital (4) releases name the rating people know.
    let content_rating = m.release_dates.as_ref().and_then(|r| {
        pick_rating(
            region,
            r.results.iter().flat_map(|country| {
                let mut dates: Vec<&RawReleaseDate> = country.release_dates.iter().collect();
                dates.sort_by_key(|d| !matches!(d.kind, Some(3 | 4)));
                dates
                    .into_iter()
                    .map(|d| (country.iso_3166_1.as_str(), d.certification.as_str()))
                    .collect::<Vec<_>>()
            }),
        )
    });
    let credits = m
        .credits
        .as_ref()
        .map(|c| Credits {
            cast: cast(&c.cast),
            guests: Vec::new(),
            crew: crew(&c.crew),
        })
        .unwrap_or_default();
    MovieData {
        provider: PROVIDER.into(),
        id: m.id.to_string(),
        year: year(&m.release_date),
        release: m.release_date.as_deref().and_then(AirTime::from_date),
        title: m.title,
        original_title: non_empty(m.original_title),
        overview: non_empty(m.overview),
        runtime_minutes: m.runtime.filter(|r| *r > 0),
        genres: m.genres.into_iter().map(|g| g.name).collect(),
        external_ids,
        poster: m.poster_path,
        backdrop: m.backdrop_path,
        tagline: non_empty(m.tagline),
        content_rating,
        credits,
        studios: companies(&m.production_companies),
        rating: rating(m.vote_average, m.vote_count),
    }
}

// ── Raw API shapes ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct Page<T> {
    results: Vec<T>,
}

#[derive(Deserialize)]
struct RawTvResult {
    id: i64,
    name: String,
    original_name: Option<String>,
    first_air_date: Option<String>,
    overview: Option<String>,
}

impl From<RawTvResult> for SearchResult {
    fn from(r: RawTvResult) -> Self {
        SearchResult {
            provider: PROVIDER.into(),
            id: r.id.to_string(),
            year: year(&r.first_air_date),
            original_title: non_empty(r.original_name).filter(|o| *o != r.name),
            title: r.name,
            overview: non_empty(r.overview),
        }
    }
}

#[derive(Deserialize)]
struct RawMovieResult {
    id: i64,
    title: String,
    original_title: Option<String>,
    release_date: Option<String>,
    overview: Option<String>,
}

impl From<RawMovieResult> for SearchResult {
    fn from(r: RawMovieResult) -> Self {
        SearchResult {
            provider: PROVIDER.into(),
            id: r.id.to_string(),
            year: year(&r.release_date),
            original_title: non_empty(r.original_title).filter(|o| *o != r.title),
            title: r.title,
            overview: non_empty(r.overview),
        }
    }
}

#[derive(Deserialize)]
struct RawFind {
    #[serde(default)]
    tv_results: Vec<RawId>,
    #[serde(default)]
    movie_results: Vec<RawId>,
}

#[derive(Deserialize)]
struct RawId {
    id: i64,
}

#[derive(Deserialize)]
pub(crate) struct RawGenre {
    name: String,
}

#[derive(Deserialize)]
pub(crate) struct RawExternalIds {
    imdb_id: Option<String>,
    tvdb_id: Option<i64>,
}

#[derive(Deserialize)]
pub(crate) struct RawShow {
    id: i64,
    name: String,
    original_name: Option<String>,
    overview: Option<String>,
    first_air_date: Option<String>,
    status: Option<String>,
    #[serde(default)]
    genres: Vec<RawGenre>,
    #[serde(default)]
    episode_run_time: Vec<i64>,
    #[serde(default)]
    seasons: Vec<RawSeasonSummary>,
    external_ids: Option<RawExternalIds>,
    episode_groups: Option<RawGroupList>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    tagline: Option<String>,
    vote_average: Option<f64>,
    vote_count: Option<i64>,
    #[serde(default)]
    created_by: Vec<RawPerson>,
    #[serde(default)]
    networks: Vec<RawCompany>,
    #[serde(default)]
    production_companies: Vec<RawCompany>,
    aggregate_credits: Option<RawAggregateCredits>,
    content_ratings: Option<RawList<RawContentRating>>,
}

#[derive(Deserialize)]
pub(crate) struct RawList<T> {
    #[serde(default = "Vec::new")]
    results: Vec<T>,
}

#[derive(Deserialize)]
pub(crate) struct RawContentRating {
    iso_3166_1: String,
    #[serde(default)]
    rating: String,
}

#[derive(Deserialize)]
pub(crate) struct RawCountryReleases {
    iso_3166_1: String,
    #[serde(default)]
    release_dates: Vec<RawReleaseDate>,
}

#[derive(Deserialize)]
pub(crate) struct RawReleaseDate {
    #[serde(default)]
    certification: String,
    #[serde(rename = "type")]
    kind: Option<i64>,
}

#[derive(Deserialize)]
pub(crate) struct RawCompany {
    id: i64,
    name: String,
}

#[derive(Deserialize)]
pub(crate) struct RawPerson {
    id: i64,
    name: String,
    profile_path: Option<String>,
    known_for_department: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct RawCast {
    #[serde(flatten)]
    person: RawPerson,
    character: Option<String>,
    order: Option<i64>,
}

#[derive(Deserialize)]
pub(crate) struct RawCrew {
    #[serde(flatten)]
    person: RawPerson,
    job: Option<String>,
    department: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct RawCredits {
    #[serde(default)]
    cast: Vec<RawCast>,
    #[serde(default)]
    crew: Vec<RawCrew>,
}

#[derive(Deserialize)]
pub(crate) struct RawAggregateCredits {
    #[serde(default)]
    cast: Vec<RawAggregateCast>,
}

#[derive(Deserialize)]
pub(crate) struct RawAggregateCast {
    #[serde(flatten)]
    person: RawPerson,
    #[serde(default)]
    roles: Vec<RawRole>,
    total_episode_count: Option<i64>,
    order: Option<i64>,
}

#[derive(Deserialize)]
pub(crate) struct RawRole {
    character: Option<String>,
}

#[derive(Deserialize)]
struct RawPersonDetail {
    id: i64,
    name: String,
    profile_path: Option<String>,
    known_for_department: Option<String>,
    biography: Option<String>,
    birthday: Option<String>,
    deathday: Option<String>,
    place_of_birth: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct RawSeasonSummary {
    season_number: i64,
}

#[derive(Deserialize)]
pub(crate) struct RawGroupList {
    results: Vec<RawGroupSummary>,
}

#[derive(Deserialize)]
pub(crate) struct RawGroupSummary {
    id: String,
    #[serde(rename = "type")]
    kind: i64,
}

#[derive(Deserialize)]
pub(crate) struct RawSeason {
    season_number: i64,
    name: Option<String>,
    overview: Option<String>,
    poster_path: Option<String>,
    #[serde(default)]
    episodes: Vec<RawEpisode>,
}

#[derive(Deserialize)]
pub(crate) struct RawEpisode {
    id: i64,
    name: Option<String>,
    overview: Option<String>,
    air_date: Option<String>,
    runtime: Option<i64>,
    season_number: i64,
    episode_number: i64,
    vote_average: Option<f64>,
    vote_count: Option<i64>,
    #[serde(default)]
    guest_stars: Vec<RawCast>,
    #[serde(default)]
    crew: Vec<RawCrew>,
}

#[derive(Deserialize)]
pub(crate) struct RawGroup {
    name: String,
    groups: Vec<RawGroupPart>,
}

#[derive(Deserialize)]
pub(crate) struct RawGroupPart {
    order: Option<i64>,
    episodes: Vec<RawGroupEpisode>,
}

#[derive(Deserialize)]
pub(crate) struct RawGroupEpisode {
    id: i64,
    order: i64,
}

#[derive(Deserialize)]
pub(crate) struct RawMovie {
    id: i64,
    title: String,
    original_title: Option<String>,
    overview: Option<String>,
    release_date: Option<String>,
    runtime: Option<i64>,
    imdb_id: Option<String>,
    #[serde(default)]
    genres: Vec<RawGenre>,
    external_ids: Option<RawExternalIds>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    tagline: Option<String>,
    vote_average: Option<f64>,
    vote_count: Option<i64>,
    #[serde(default)]
    production_companies: Vec<RawCompany>,
    credits: Option<RawCredits>,
    release_dates: Option<RawList<RawCountryReleases>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse<T: DeserializeOwned>(json: &str) -> T {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn maps_a_show_with_seasons_and_an_absolute_group() {
        let show: RawShow = parse(
            r#"{"id": 93405, "name": "Kitsune Road", "original_name": "Kitsune no Michi",
                "overview": "A fox spirit.", "first_air_date": "2019-04-04", "status": "Returning Series",
                "genres": [{"id": 16, "name": "Animation"}], "episode_run_time": [24],
                "seasons": [{"season_number": 0}, {"season_number": 1}],
                "external_ids": {"imdb_id": "tt9208876", "tvdb_id": 361205},
                "episode_groups": {"results": [{"id": "abc", "type": 2}]},
                "poster_path": "/p.jpg", "backdrop_path": "/b.jpg"}"#,
        );
        let specials: RawSeason = parse(
            r#"{"season_number": 0, "name": "Specials", "episodes": [
                {"id": 900, "name": "Recap", "air_date": "2019-06-01", "season_number": 0, "episode_number": 1}]}"#,
        );
        let s1: RawSeason = parse(
            r#"{"season_number": 1, "name": "Season 1", "overview": "", "episodes": [
                {"id": 101, "name": "The Fox at the Crossing", "air_date": "2019-04-04", "runtime": 25,
                 "season_number": 1, "episode_number": 1},
                {"id": 102, "name": "", "air_date": null, "season_number": 1, "episode_number": 2}]}"#,
        );
        let group: RawGroup = parse(
            r#"{"name": "Absolute", "groups": [{"order": 1, "episodes": [
                {"id": 102, "order": 1}, {"id": 101, "order": 0}]}]}"#,
        );

        let data = series_from(show, vec![specials, s1], vec![("absolute", group)], "US");

        assert_eq!((data.id.as_str(), data.year), ("93405", Some(2019)));
        assert_eq!(data.status, Some(SeriesStatus::Continuing));
        assert_eq!(data.genres, ["Animation"]);
        assert_eq!(
            data.external_ids,
            [
                ("tvdb".into(), "361205".into()),
                ("imdb".into(), "tt9208876".into())
            ]
        );
        assert_eq!(data.episodes.len(), 3);
        let pilot = &data.episodes[1];
        assert_eq!(
            (pilot.season, pilot.number, pilot.runtime_minutes),
            (1, 1, Some(25))
        );
        // TMDB has dates, not times: the episode counts as aired at the end of that day, UTC.
        let airs = pilot.airs.unwrap();
        assert!(!airs.time_known);
        assert_eq!(airs.at, 1_554_422_400, "2019-04-05T00:00:00Z");
        assert_eq!(data.episodes[2].title, "Episode 2");
        assert_eq!(
            data.episodes[2].runtime_minutes,
            Some(24),
            "series runtime as a fallback"
        );
        assert_eq!(data.seasons[1].overview, None);

        let absolute = &data.orderings[0];
        assert_eq!(absolute.kind, "absolute");
        let numbers: Vec<(&str, Option<i64>, i64)> = absolute
            .entries
            .iter()
            .map(|e| (e.episode_id.as_str(), e.season, e.number))
            .collect();
        assert_eq!(numbers, [("101", None, 1), ("102", None, 2)]);
    }

    #[test]
    fn maps_a_movie() {
        let movie: RawMovie = parse(
            r#"{"id": 99020, "title": "The Quiet Meridian", "original_title": "The Quiet Meridian",
                "release_date": "2014-09-12", "runtime": 118, "imdb_id": "tt9902000",
                "genres": [{"id": 18, "name": "Drama"}]}"#,
        );
        let m = movie_from(movie, "US");
        assert_eq!((m.year, m.runtime_minutes), (Some(2014), Some(118)));
        assert_eq!(
            m.external_ids,
            [("imdb".to_string(), "tt9902000".to_string())]
        );
    }

    #[test]
    fn maps_movie_credits_companies_and_ratings() {
        let movie: RawMovie = parse(
            r#"{"id": 1, "title": "Harbour Lights", "tagline": "Every light is a promise.",
                "vote_average": 7.4, "vote_count": 812,
                "production_companies": [{"id": 7, "name": "Lantern Films"}],
                "release_dates": {"results": [
                    {"iso_3166_1": "DE", "release_dates": [{"certification": "12", "type": 3}]},
                    {"iso_3166_1": "US", "release_dates": [
                        {"certification": "", "type": 1}, {"certification": "PG-13", "type": 3}]}]},
                "credits": {
                    "cast": [
                        {"id": 20, "name": "Bo Second", "character": "Ferry", "order": 1},
                        {"id": 10, "name": "Ada First", "character": "Keeper", "order": 0,
                         "profile_path": "/ada.jpg", "known_for_department": "Acting"},
                        {"id": 10, "name": "Ada First", "character": "Keeper (young)", "order": 5}],
                    "crew": [
                        {"id": 30, "name": "Cy Grip", "job": "Key Grip", "department": "Crew"},
                        {"id": 40, "name": "Di Pen", "job": "Screenplay", "department": "Writing"},
                        {"id": 40, "name": "Di Pen", "job": "Director", "department": "Directing"},
                        {"id": 40, "name": "Di Pen", "job": "Director", "department": "Directing"}]}}"#,
        );
        let m = movie_from(movie, "US");
        assert_eq!(m.tagline.as_deref(), Some("Every light is a promise."));
        assert_eq!(m.content_rating.as_deref(), Some("PG-13"));
        assert_eq!(m.studios[0].name, "Lantern Films");
        assert_eq!(
            m.rating,
            Some(RatingData {
                value: 7.4,
                scale: 10.0,
                votes: Some(812)
            })
        );
        let cast: Vec<(&str, Option<&str>)> = m
            .credits
            .cast
            .iter()
            .map(|c| (c.person.name.as_str(), c.role.as_deref()))
            .collect();
        assert_eq!(
            cast,
            [("Ada First", Some("Keeper")), ("Bo Second", Some("Ferry"))]
        );
        assert_eq!(m.credits.cast[0].person.photo.as_deref(), Some("/ada.jpg"));
        let crew: Vec<(&str, Option<&str>)> = m
            .credits
            .crew
            .iter()
            .map(|c| (c.person.name.as_str(), c.role.as_deref()))
            .collect();
        assert_eq!(
            crew,
            [("Di Pen", Some("Director")), ("Di Pen", Some("Screenplay"))],
            "key jobs only, director first, no duplicates"
        );

        let german = movie_from(
            parse(
                r#"{"id": 1, "title": "Harbour Lights", "release_dates": {"results": [
                    {"iso_3166_1": "DE", "release_dates": [{"certification": "12", "type": 3}]}]}}"#,
            ),
            "DE",
        );
        assert_eq!(german.content_rating.as_deref(), Some("12"));
        assert_eq!(german.rating, None, "no votes, no score");
    }

    #[test]
    fn maps_series_cast_creators_networks_and_episode_guests() {
        let show: RawShow = parse(
            r#"{"id": 5, "name": "Night Harbor", "seasons": [{"season_number": 1}],
                "created_by": [{"id": 1, "name": "Ma Maker"}],
                "networks": [{"id": 49, "name": "HBO"}],
                "content_ratings": {"results": [{"iso_3166_1": "US", "rating": "TV-MA"}]},
                "aggregate_credits": {"cast": [
                    {"id": 2, "name": "Lead", "order": 0, "total_episode_count": 20,
                     "roles": [{"character": "Captain"}, {"character": "Captain (voice)"}]}]}}"#,
        );
        let season: RawSeason = parse(
            r#"{"season_number": 1, "episodes": [{"id": 9, "name": "Pilot", "season_number": 1,
                "episode_number": 1, "vote_average": 8.0, "vote_count": 3,
                "guest_stars": [{"id": 3, "name": "Guest", "character": "Stranger", "order": 0}],
                "crew": [{"id": 4, "name": "Dee", "job": "Director", "department": "Directing"},
                         {"id": 5, "name": "Wri", "job": "Writer", "department": "Writing"}]}]}"#,
        );
        let data = series_from(show, vec![season], vec![], "US");
        assert_eq!(data.content_rating.as_deref(), Some("TV-MA"));
        assert_eq!(data.networks[0].name, "HBO");
        let lead = &data.credits.cast[0];
        assert_eq!(lead.role.as_deref(), Some("Captain / Captain (voice)"));
        assert_eq!(lead.episodes, Some(20));
        assert_eq!(data.credits.crew[0].role.as_deref(), Some("Creator"));
        let pilot = &data.episodes[0];
        assert_eq!(pilot.credits.guests[0].person.name, "Guest");
        assert_eq!(pilot.credits.crew.len(), 2);
        assert_eq!(pilot.rating.map(|r| r.value), Some(8.0));
    }
}
