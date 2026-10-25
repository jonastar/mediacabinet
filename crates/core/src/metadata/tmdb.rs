//! TMDB (themoviedb.org) API v3.
//!
//! Accepts either a v3 API key or a v4 read access token. Episode groups become
//! orderings: TMDB's group types 2 (absolute), 3 (DVD) and 5 (story arc) map to
//! ours; the rest become custom orderings.

use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::MetadataError;
use super::model::*;

const BASE: &str = "https://api.themoviedb.org/3";
pub const PROVIDER: &str = "tmdb";

#[derive(Clone)]
pub struct Tmdb {
    http: reqwest::Client,
    key: String,
    base: String,
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
        }
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
                    "external_ids,episode_groups".to_string(),
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
        Ok(series_from(show, seasons, groups))
    }

    pub async fn movie(&self, id: &str) -> Result<MovieData, MetadataError> {
        let movie: RawMovie = self
            .get(
                &format!("/movie/{id}"),
                &[("append_to_response", "external_ids".to_string())],
            )
            .await?;
        Ok(movie_from(movie))
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

pub(crate) fn series_from(
    show: RawShow,
    seasons: Vec<RawSeason>,
    groups: Vec<(&str, RawGroup)>,
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
    }
}

pub(crate) fn movie_from(m: RawMovie) -> MovieData {
    let mut external_ids = Vec::new();
    let imdb = m.imdb_id.or(m.external_ids.and_then(|e| e.imdb_id));
    if let Some(imdb) = imdb.filter(|s| !s.is_empty()) {
        external_ids.push(("imdb".to_string(), imdb));
    }
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

        let data = series_from(show, vec![specials, s1], vec![("absolute", group)]);

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
        let m = movie_from(movie);
        assert_eq!((m.year, m.runtime_minutes), (Some(2014), Some(118)));
        assert_eq!(
            m.external_ids,
            [("imdb".to_string(), "tt9902000".to_string())]
        );
    }
}
