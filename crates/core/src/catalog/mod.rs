//! Read models for the UI: series, their episodes in an ordering or playlist, and
//! the home screen. Derived state is computed here from links, file status and
//! dates (see `rules`), never stored.

pub mod movies;
pub mod people;
pub mod rules;

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde_json::json;
use sqlx::{SqliteConnection, SqlitePool};
use utoipa::ToSchema;

pub use rules::Availability;

/// Episodes airing within this window count as "coming up".
const COMING_UP_SECONDS: i64 = 7 * 24 * 3600;
/// Files first seen within this window make an episode "new".
const NEW_SECONDS: i64 = 7 * 24 * 3600;
/// Custom statuses that mean an external tool is working on an item.
const IN_PROGRESS_STATUSES: &[&str] = &["searching", "queued", "downloading", "importing"];

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("not found")]
    NotFound,
    #[error("invalid view {0:?}; expected ordering:<id> or playlist:<id> of this series")]
    InvalidView(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// An ordering or playlist, addressed as `ordering:<id>` or `playlist:<id>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewRef {
    Ordering(i64),
    Playlist(i64),
}

impl ViewRef {
    pub fn parse(s: &str) -> Option<ViewRef> {
        let (kind, id) = s.split_once(':')?;
        let id = id.parse().ok()?;
        match kind {
            "ordering" => Some(ViewRef::Ordering(id)),
            "playlist" => Some(ViewRef::Playlist(id)),
            _ => None,
        }
    }

    pub fn key(self) -> String {
        match self {
            ViewRef::Ordering(id) => format!("ordering:{id}"),
            ViewRef::Playlist(id) => format!("playlist:{id}"),
        }
    }
}

// ── API shapes ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NumberStyle {
    /// `S01E03`
    Season,
    /// `03`
    Absolute,
    /// `E03`, or `S01E03` when the playlist's base ordering has seasons.
    Playlist,
}

/// An episode's number in one ordering or playlist. Formatting is up to the client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct EpisodeNumber {
    pub style: NumberStyle,
    pub season: Option<i64>,
    pub number: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct StateCounts {
    pub available: i64,
    pub missing: i64,
    pub not_acquired: i64,
    pub upcoming: i64,
    pub unknown: i64,
    pub wanted: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EpisodeRef {
    pub item_id: i64,
    pub title: String,
    pub number: Option<EpisodeNumber>,
    /// Unix seconds, UTC.
    pub airs_at: Option<i64>,
    /// False when the provider only gave a date: show the date without a time.
    pub airs_time_known: bool,
    pub runtime_seconds: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SeriesSummary {
    pub id: i64,
    /// The library it belongs to; null only if it predates having one.
    pub library_id: Option<i64>,
    pub title: String,
    pub year: Option<i64>,
    /// `continuing`, `ended` or `cancelled`.
    pub series_status: Option<String>,
    pub counts: StateCounts,
    pub next_airing: Option<EpisodeRef>,
    /// Available, unplayed episodes whose file arrived in the last 7 days.
    pub new_episodes: i64,
    pub genres: Vec<String>,
    /// Artwork kinds available from `/api/artwork/{id}/{kind}`: poster, backdrop.
    pub artwork: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct View {
    /// `ordering:<id>` or `playlist:<id>`.
    pub id: String,
    pub name: String,
    /// Ordering kind (`aired`, `dvd`, `absolute`, `story`, `custom`) or `playlist`.
    pub kind: String,
    /// Where it comes from: a provider (`tmdb`), `user`, or `smart`/`manual` for playlists.
    pub source: String,
    pub is_default: bool,
    /// Playlists: visible to every user.
    pub shared: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ExternalId {
    pub provider: String,
    pub value: String,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QualityProfile {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Location {
    pub root_id: i64,
    pub root_path: String,
    /// Relative to the root.
    pub path: String,
    pub is_primary: bool,
    pub write_policy: String,
    /// Whether the root folder is reachable right now.
    pub online: bool,
    pub file_count: i64,
    /// The path with the provider IDs in the folder's name corrected, when they
    /// disagree with the series' own (a wrong `[tmdb-…]` tag, say).
    pub corrected_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SeriesDetail {
    #[serde(flatten)]
    pub summary: SeriesSummary,
    pub overview: Option<String>,
    pub primary_provider: Option<String>,
    pub external_ids: Vec<ExternalId>,
    /// The series' own setting; episodes inherit it unless set themselves.
    pub monitored: bool,
    pub quality_profile_id: Option<i64>,
    pub quality_profiles: Vec<QualityProfile>,
    pub views: Vec<View>,
    /// The current user's view, or the default ordering.
    pub selected_view: Option<String>,
    pub locations: Vec<Location>,
    /// The playlist wanted follows (`playlist:<id>`), or none for all episodes.
    pub wanted_follows: Option<String>,
    /// Fields metadata refreshes never overwrite.
    pub locked_fields: Vec<String>,
    /// The view filenames are numbered in, if set; otherwise the default first.
    pub match_view: Option<String>,
    pub refreshed_at: Option<i64>,
    pub refresh_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LinkedFile {
    pub file_id: i64,
    pub root_path: String,
    pub relative_path: String,
    /// `present`, `missing` or `ignored`.
    pub status: String,
    pub size: i64,
    pub last_seen: i64,
    pub container: Option<String>,
    pub video_codec: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub hdr_format: Option<String>,
    pub edition: Option<String>,
    /// For movies split over several files.
    pub part: Option<i64>,
    /// How it was matched: `path_id`, `nfo`, `parser`, `ai` or `user`.
    pub link_source: String,
    pub confidence: f64,
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ExternalStatus {
    pub source: String,
    pub status: String,
    #[schema(value_type = Option<Object>)]
    pub detail: Option<serde_json::Value>,
    pub updated_at: i64,
    pub expires_at: i64,
    /// Past its expiry: the tool stopped reporting.
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WatchState {
    pub position_seconds: f64,
    pub played: bool,
    pub last_played_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Episode {
    #[serde(flatten)]
    pub episode: EpisodeRef,
    pub availability: Availability,
    pub monitored: bool,
    pub wanted: bool,
    /// Why it's wanted: `missing`, `not_acquired` or `upgrade` (below the quality cutoff).
    pub wanted_reason: Option<String>,
    pub overview: Option<String>,
    pub is_filler: bool,
    pub is_gap: bool,
    pub watch: Option<WatchState>,
    pub files: Vec<LinkedFile>,
    pub statuses: Vec<ExternalStatus>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EpisodeList {
    pub view: Option<String>,
    pub entries: Vec<Episode>,
    /// Index into `entries`.
    pub up_next: Option<usize>,
    /// Over this view's entries.
    pub counts: StateCounts,
    pub gaps: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ContinueItem {
    pub series_id: i64,
    pub series_title: String,
    pub view_name: Option<String>,
    pub episode: EpisodeRef,
    pub position_seconds: f64,
    pub last_played_at: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ComingUp {
    pub series_id: i64,
    pub series_title: String,
    pub episode: EpisodeRef,
}

#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct Attention {
    pub review: i64,
    pub missing_files: i64,
    pub offline_roots: Vec<String>,
    pub wanted: i64,
    /// Wanted items an external tool is working on.
    pub in_progress: i64,
    pub status_sources: Vec<String>,
    pub gaps: i64,
    pub gap_series: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Home {
    /// The most recently watched series' next episode.
    pub hero: Option<ContinueItem>,
    pub keep_watching: Vec<ContinueItem>,
    pub coming_up: Vec<ComingUp>,
    pub shows: Vec<SeriesSummary>,
    pub movies: Vec<movies::MovieSummary>,
    pub attention: Attention,
}

// ── Loading ────────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct SeriesRow {
    id: i64,
    library_id: Option<i64>,
    title: String,
    year: Option<i64>,
    overview: Option<String>,
    series_status: Option<String>,
    primary_provider: Option<String>,
    monitored: Option<bool>,
    quality_profile_id: Option<i64>,
    metadata_locked: String,
    match_ordering_id: Option<i64>,
    wanted_playlist_id: Option<i64>,
    refreshed_at: Option<i64>,
    refresh_error: Option<String>,
}

#[derive(sqlx::FromRow, Clone)]
struct EpisodeRow {
    id: i64,
    title: String,
    airs_at: Option<i64>,
    airs_tz: Option<String>,
    runtime_seconds: Option<i64>,
    /// From the `item_state` and `item_wanted` views.
    availability: String,
    monitored: bool,
    wanted_reason: Option<String>,
    position_seconds: Option<f64>,
    played: Option<bool>,
    last_played_at: Option<i64>,
    is_filler: bool,
    newest_file: Option<i64>,
    overview: Option<String>,
}

/// One series with all its episodes and the user's state, loaded once.
struct Loaded {
    series: SeriesRow,
    genres: Vec<String>,
    artwork: Vec<String>,
    episodes: HashMap<i64, EpisodeRow>,
    tags: HashMap<i64, HashSet<String>>,
    now: i64,
}

impl Loaded {
    async fn load(
        conn: &mut SqliteConnection,
        series_id: i64,
        user: Option<i64>,
        now: i64,
    ) -> Result<Loaded, CatalogError> {
        let series: SeriesRow = sqlx::query_as(
            "SELECT i.id, i.library_id, i.title, i.year, i.overview, s.status AS series_status,
                    i.primary_provider, m.monitored, m.quality_profile_id, i.metadata_locked,
                    s.match_ordering_id, s.wanted_playlist_id, i.refreshed_at, i.refresh_error
             FROM series s JOIN item i ON i.id = s.item_id
             LEFT JOIN monitor m ON m.item_id = i.id
             WHERE s.item_id = ?",
        )
        .bind(series_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(CatalogError::NotFound)?;

        let episodes: Vec<EpisodeRow> = sqlx::query_as(
            "SELECT e.id, e.title, e.overview, e.airs_at, e.airs_tz, e.runtime_seconds,
                    st.availability, st.monitored, w.reason AS wanted_reason,
                    s.position_seconds, s.played, s.last_played_at,
                    EXISTS (SELECT 1 FROM item_tag t WHERE t.item_id = e.id AND t.tag = 'filler')
                        AS is_filler,
                    (SELECT max(f.first_seen) FROM file_link l JOIN media_file f ON f.id = l.file_id
                     WHERE l.item_id = e.id AND f.status = 'present') AS newest_file
             FROM item e
             JOIN item_state st ON st.item_id = e.id
             LEFT JOIN item_wanted w ON w.item_id = e.id
             LEFT JOIN user_item_state s ON s.item_id = e.id AND s.user_id = ?
             WHERE e.parent_id = ?",
        )
        .bind(user)
        .bind(series_id)
        .fetch_all(&mut *conn)
        .await?;

        let tag_rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT t.item_id, t.tag FROM item_tag t JOIN item e ON e.id = t.item_id
             WHERE e.parent_id = ?",
        )
        .bind(series_id)
        .fetch_all(&mut *conn)
        .await?;
        let mut tags: HashMap<i64, HashSet<String>> = HashMap::new();
        for (id, tag) in tag_rows {
            tags.entry(id).or_default().insert(tag);
        }

        let (genres, artwork) = genres_and_artwork(&mut *conn, series_id).await?;
        Ok(Loaded {
            series,
            genres,
            artwork,
            episodes: episodes.into_iter().map(|e| (e.id, e)).collect(),
            tags,
            now,
        })
    }

    fn availability(&self, e: &EpisodeRow) -> Availability {
        Availability::parse(&e.availability)
    }

    fn monitored(&self, e: &EpisodeRow) -> bool {
        e.monitored
    }

    fn played(e: &EpisodeRow) -> bool {
        e.played.unwrap_or(false)
    }

    fn counts<'a>(&self, episodes: impl Iterator<Item = &'a EpisodeRow>) -> StateCounts {
        let mut c = StateCounts::default();
        for e in episodes {
            let a = self.availability(e);
            match a {
                Availability::Available => c.available += 1,
                Availability::Missing => c.missing += 1,
                Availability::NotAcquired => c.not_acquired += 1,
                Availability::Upcoming => c.upcoming += 1,
                Availability::Unknown => c.unknown += 1,
            }
            if e.wanted_reason.is_some() {
                c.wanted += 1;
            }
            c.total += 1;
        }
        c
    }

    fn episode_ref(&self, e: &EpisodeRow, number: Option<EpisodeNumber>) -> EpisodeRef {
        EpisodeRef {
            item_id: e.id,
            title: e.title.clone(),
            number,
            airs_at: e.airs_at,
            airs_time_known: e.airs_tz.is_some(),
            runtime_seconds: e.runtime_seconds,
        }
    }

    /// The earliest upcoming episode, numbered as in `numbers`.
    fn next_airing(&self, numbers: &HashMap<i64, EpisodeNumber>) -> Option<EpisodeRef> {
        self.episodes
            .values()
            .filter(|e| e.airs_at.is_some_and(|t| t > self.now))
            .min_by_key(|e| (e.airs_at, e.id))
            .map(|e| self.episode_ref(e, numbers.get(&e.id).cloned()))
    }

    fn summary(&self, numbers: &HashMap<i64, EpisodeNumber>) -> SeriesSummary {
        let new_episodes = self
            .episodes
            .values()
            .filter(|e| {
                self.availability(e) == Availability::Available
                    && !Self::played(e)
                    && e.newest_file.is_some_and(|t| t >= self.now - NEW_SECONDS)
            })
            .count() as i64;
        SeriesSummary {
            id: self.series.id,
            library_id: self.series.library_id,
            title: self.series.title.clone(),
            year: self.series.year,
            series_status: self.series.series_status.clone(),
            counts: self.counts(self.episodes.values()),
            next_airing: self.next_airing(numbers),
            new_episodes,
            genres: self.genres.clone(),
            artwork: self.artwork.clone(),
        }
    }
}

async fn genres_and_artwork(
    conn: &mut SqliteConnection,
    item: i64,
) -> Result<(Vec<String>, Vec<String>), sqlx::Error> {
    let genres: Vec<String> = sqlx::query_scalar(
        "SELECT substr(tag, 7) FROM item_tag WHERE item_id = ? AND tag LIKE 'genre:%' ORDER BY rowid",
    )
    .bind(item)
    .fetch_all(&mut *conn)
    .await?;
    let artwork: Vec<String> =
        sqlx::query_scalar("SELECT kind FROM item_artwork WHERE item_id = ? ORDER BY kind")
            .bind(item)
            .fetch_all(&mut *conn)
            .await?;
    Ok((genres, artwork))
}

/// Specials (season 0) are left out of gaps: owning them says nothing about the
/// regular episodes around them.
fn counts_for_gaps(entries: &[(i64, Option<EpisodeNumber>)]) -> Vec<bool> {
    entries
        .iter()
        .map(|(_, n)| n.as_ref().is_none_or(|n| n.season != Some(0)))
        .collect()
}

/// A view resolved to its entries, in order.
struct Resolved {
    view: Option<View>,
    entries: Vec<(i64, Option<EpisodeNumber>)>,
}

impl Resolved {
    fn numbers(&self) -> HashMap<i64, EpisodeNumber> {
        let mut numbers = HashMap::new();
        for (id, n) in &self.entries {
            if let Some(n) = n {
                numbers.entry(*id).or_insert_with(|| n.clone());
            }
        }
        numbers
    }
}

async fn views(
    conn: &mut SqliteConnection,
    series_id: i64,
    user: Option<i64>,
) -> Result<Vec<View>, sqlx::Error> {
    let orderings: Vec<(i64, String, String, String, bool)> = sqlx::query_as(
        "SELECT id, name, kind, source, is_default FROM ordering
         WHERE series_id = ? ORDER BY is_default DESC, id",
    )
    .bind(series_id)
    .fetch_all(&mut *conn)
    .await?;
    let playlists: Vec<(i64, String, String, Option<i64>)> = sqlx::query_as(
        "SELECT DISTINCT p.id, p.name, p.kind, p.owner_user_id FROM playlist p
         LEFT JOIN ordering o ON o.id = p.base_ordering_id
         LEFT JOIN playlist_entry pe ON pe.playlist_id = p.id
         LEFT JOIN item i ON i.id = pe.item_id
         WHERE (o.series_id = ?1 OR i.parent_id = ?1)
           AND (p.owner_user_id IS NULL OR p.owner_user_id = ?2)
         ORDER BY p.id",
    )
    .bind(series_id)
    .bind(user)
    .fetch_all(&mut *conn)
    .await?;

    let mut views: Vec<View> = orderings
        .into_iter()
        .map(|(id, name, kind, source, is_default)| View {
            id: ViewRef::Ordering(id).key(),
            name,
            kind,
            source,
            is_default,
            shared: true,
        })
        .collect();
    views.extend(playlists.into_iter().map(|(id, name, kind, owner)| View {
        id: ViewRef::Playlist(id).key(),
        name,
        kind: "playlist".into(),
        source: kind,
        is_default: false,
        shared: owner.is_none(),
    }));
    Ok(views)
}

async fn user_view(
    conn: &mut SqliteConnection,
    series_id: i64,
    user: Option<i64>,
) -> Result<Option<ViewRef>, sqlx::Error> {
    let pref: Option<(Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT ordering_id, playlist_id FROM user_series_pref WHERE user_id = ? AND series_id = ?",
    )
    .bind(user)
    .bind(series_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(match pref {
        Some((Some(id), _)) => Some(ViewRef::Ordering(id)),
        Some((_, Some(id))) => Some(ViewRef::Playlist(id)),
        _ => None,
    })
}

async fn ordering_entries(
    conn: &mut SqliteConnection,
    ordering_id: i64,
    style_override: Option<NumberStyle>,
) -> Result<Vec<(i64, Option<EpisodeNumber>)>, sqlx::Error> {
    let kind: String = sqlx::query_scalar("SELECT kind FROM ordering WHERE id = ?")
        .bind(ordering_id)
        .fetch_one(&mut *conn)
        .await?;
    let rows: Vec<(i64, Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT oe.item_id, os.number, oe.episode_number FROM ordering_entry oe
         LEFT JOIN ordering_season os ON os.id = oe.ordering_season_id
         WHERE oe.ordering_id = ? ORDER BY oe.position",
    )
    .bind(ordering_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, season, number)| {
            let style = style_override.unwrap_or(if kind == "absolute" || season.is_none() {
                NumberStyle::Absolute
            } else {
                NumberStyle::Season
            });
            let season = if kind == "absolute" {
                None
            } else {
                season
            };
            (
                id,
                Some(EpisodeNumber {
                    style,
                    season,
                    number,
                }),
            )
        })
        .collect())
}

/// Resolves `view` (or the user's, or the default ordering) to entries of this series.
async fn resolve(
    conn: &mut SqliteConnection,
    loaded: &Loaded,
    views: &[View],
    view: Option<ViewRef>,
) -> Result<Resolved, CatalogError> {
    let wanted = view.map(|v| v.key());
    let chosen = match &wanted {
        Some(key) => Some(
            views
                .iter()
                .find(|v| &v.id == key)
                .ok_or_else(|| CatalogError::InvalidView(key.clone()))?,
        ),
        None => views.iter().find(|v| v.is_default).or(views.first()),
    };
    let Some(chosen) = chosen.cloned() else {
        // No orderings: every episode by air date, unnumbered.
        let mut episodes: Vec<&EpisodeRow> = loaded.episodes.values().collect();
        episodes.sort_by_key(|e| (e.airs_at.is_none(), e.airs_at, e.id));
        return Ok(Resolved {
            view: None,
            entries: episodes.into_iter().map(|e| (e.id, None)).collect(),
        });
    };

    let entries = match ViewRef::parse(&chosen.id).expect("views have valid ids") {
        ViewRef::Ordering(id) => ordering_entries(conn, id, None).await?,
        ViewRef::Playlist(id) => {
            let (kind, base, rules): (String, Option<i64>, Option<String>) =
                sqlx::query_as("SELECT kind, base_ordering_id, rules FROM playlist WHERE id = ?")
                    .bind(id)
                    .fetch_one(&mut *conn)
                    .await?;
            if kind == "smart" {
                let rules: serde_json::Value = rules
                    .and_then(|r| serde_json::from_str(&r).ok())
                    .unwrap_or_default();
                let excluded: HashSet<&str> = rules["exclude_tags"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|t| t.as_str()).collect())
                    .unwrap_or_default();
                let base = base.expect("smart playlists have a base ordering");
                ordering_entries(conn, base, Some(NumberStyle::Playlist))
                    .await?
                    .into_iter()
                    .filter(|(item, _)| {
                        loaded
                            .tags
                            .get(item)
                            .is_none_or(|tags| tags.iter().all(|t| !excluded.contains(t.as_str())))
                    })
                    .collect()
            } else {
                let items: Vec<i64> = sqlx::query_scalar(
                    "SELECT item_id FROM playlist_entry WHERE playlist_id = ? ORDER BY position",
                )
                .bind(id)
                .fetch_all(&mut *conn)
                .await?;
                let default = views
                    .iter()
                    .find(|v| v.is_default)
                    .and_then(|v| ViewRef::parse(&v.id));
                let numbers = match default {
                    Some(ViewRef::Ordering(o)) => {
                        ordering_entries(conn, o, Some(NumberStyle::Playlist))
                            .await?
                            .into_iter()
                            .collect::<HashMap<_, _>>()
                    }
                    _ => HashMap::new(),
                };
                items
                    .into_iter()
                    .filter(|id| loaded.episodes.contains_key(id))
                    .map(|id| (id, numbers.get(&id).cloned().flatten()))
                    .collect()
            }
        }
    };
    Ok(Resolved {
        view: Some(chosen),
        entries: entries
            .into_iter()
            .filter(|(id, _)| loaded.episodes.contains_key(id))
            .collect(),
    })
}

// ── Queries ────────────────────────────────────────────────────────────────

/// Creates an admin user if there are none, so per-user state (views, watch
/// progress) works before there are logins. Returns the user's ID.
pub async fn ensure_default_user(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    if let Some(id) = default_user(pool).await? {
        return Ok(id);
    }
    sqlx::query_scalar("INSERT INTO app_user (name, is_admin) VALUES ('admin', 1) RETURNING id")
        .fetch_one(pool)
        .await
}

/// The user per-user state belongs to until there are logins: the first admin.
pub async fn default_user(pool: &SqlitePool) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM app_user ORDER BY is_admin DESC, id LIMIT 1")
        .fetch_optional(pool)
        .await
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Series by title, all or those in `library`.
async fn series_ids(
    conn: &mut SqliteConnection,
    library: Option<i64>,
) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM item WHERE kind = 'series' AND (?1 IS NULL OR library_id = ?1)
         ORDER BY coalesce(sort_title, title) COLLATE NOCASE, id",
    )
    .bind(library)
    .fetch_all(conn)
    .await
}

/// All series, or those in `library`.
pub async fn list_series(
    pool: &SqlitePool,
    user: Option<i64>,
    now: i64,
    library: Option<i64>,
) -> Result<Vec<SeriesSummary>, CatalogError> {
    let mut conn = pool.acquire().await?;
    let mut out = Vec::new();
    for id in series_ids(&mut conn, library).await? {
        let loaded = Loaded::load(&mut conn, id, user, now).await?;
        let views = views(&mut conn, id, user).await?;
        let pref = user_view(&mut conn, id, user).await?;
        let resolved = resolve(&mut conn, &loaded, &views, pref).await?;
        out.push(loaded.summary(&resolved.numbers()));
    }
    Ok(out)
}

pub async fn series_detail(
    pool: &SqlitePool,
    series_id: i64,
    user: Option<i64>,
    now: i64,
) -> Result<SeriesDetail, CatalogError> {
    let mut conn = pool.acquire().await?;
    let loaded = Loaded::load(&mut conn, series_id, user, now).await?;
    let views = views(&mut conn, series_id, user).await?;
    let pref = user_view(&mut conn, series_id, user).await?;
    let resolved = resolve(&mut conn, &loaded, &views, pref).await?;

    let external_ids: Vec<(String, String, bool)> = sqlx::query_as(
        "SELECT provider, value, is_primary FROM external_id WHERE item_id = ?
         ORDER BY is_primary DESC, provider",
    )
    .bind(series_id)
    .fetch_all(&mut *conn)
    .await?;
    let quality_profiles: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, name FROM quality_profile ORDER BY id")
            .fetch_all(&mut *conn)
            .await?;
    let location_rows: Vec<(i64, String, String, bool, String, i64)> = sqlx::query_as(
        "SELECT sl.root_id, r.path, sl.path, sl.is_primary, r.write_policy,
                (SELECT count(DISTINCT f.id) FROM media_file f
                 JOIN file_link l ON l.file_id = f.id
                 JOIN item e ON e.id = l.item_id
                 WHERE f.root_id = sl.root_id AND e.parent_id = sl.series_item_id
                   AND f.status <> 'removed')
         FROM series_location sl JOIN library_root r ON r.id = sl.root_id
         WHERE sl.series_item_id = ?
         ORDER BY sl.is_primary DESC, r.path",
    )
    .bind(series_id)
    .fetch_all(&mut *conn)
    .await?;
    let ids: Vec<(String, String)> = external_ids
        .iter()
        .map(|(provider, value, _)| (provider.clone(), value.clone()))
        .collect();
    let mut locations = Vec::new();
    for (root_id, root_path, path, is_primary, write_policy, file_count) in location_rows {
        let online = tokio::fs::metadata(&root_path)
            .await
            .is_ok_and(|m| m.is_dir());
        locations.push(Location {
            corrected_path: crate::organize::corrected_folder(&path, &ids),
            root_id,
            root_path,
            path,
            is_primary,
            write_policy,
            online,
            file_count,
        });
    }

    Ok(SeriesDetail {
        summary: loaded.summary(&resolved.numbers()),
        overview: loaded.series.overview.clone(),
        primary_provider: loaded.series.primary_provider.clone(),
        external_ids: external_ids
            .into_iter()
            .map(|(provider, value, is_primary)| ExternalId {
                provider,
                value,
                is_primary,
            })
            .collect(),
        monitored: loaded.series.monitored.unwrap_or(false),
        quality_profile_id: loaded.series.quality_profile_id,
        quality_profiles: quality_profiles
            .into_iter()
            .map(|(id, name)| QualityProfile { id, name })
            .collect(),
        selected_view: resolved.view.map(|v| v.id),
        views,
        locations,
        wanted_follows: loaded
            .series
            .wanted_playlist_id
            .map(|p| ViewRef::Playlist(p).key()),
        locked_fields: serde_json::from_str(&loaded.series.metadata_locked).unwrap_or_default(),
        match_view: loaded
            .series
            .match_ordering_id
            .map(|o| ViewRef::Ordering(o).key()),
        refreshed_at: loaded.series.refreshed_at,
        refresh_error: loaded.series.refresh_error.clone(),
    })
}

/// The series' episodes in `view`, or the user's view, or the default ordering.
pub async fn episodes(
    pool: &SqlitePool,
    series_id: i64,
    view: Option<ViewRef>,
    user: Option<i64>,
    now: i64,
) -> Result<EpisodeList, CatalogError> {
    let mut conn = pool.acquire().await?;
    let loaded = Loaded::load(&mut conn, series_id, user, now).await?;
    let views = views(&mut conn, series_id, user).await?;
    let view = match view {
        Some(v) => Some(v),
        None => user_view(&mut conn, series_id, user).await?,
    };
    let resolved = resolve(&mut conn, &loaded, &views, view).await?;

    let file_rows: Vec<FileRow> = sqlx::query_as(
        "SELECT l.item_id, f.id AS file_id, r.path AS root_path, f.relative_path, f.status,
                f.size, f.last_seen, f.container, v.codec AS video_codec, v.width, v.height,
                v.hdr_format, l.edition, l.part, l.source AS link_source, l.confidence, l.pinned
         FROM file_link l
         JOIN media_file f ON f.id = l.file_id
         JOIN library_root r ON r.id = f.root_id
         JOIN item e ON e.id = l.item_id
         LEFT JOIN media_stream v ON v.file_id = f.id AND v.stream_index =
             (SELECT min(stream_index) FROM media_stream WHERE file_id = f.id AND kind = 'video')
         WHERE e.parent_id = ? AND f.status <> 'removed'
         ORDER BY l.item_id, f.status = 'present' DESC, v.height DESC, f.id",
    )
    .bind(series_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut files: HashMap<i64, Vec<LinkedFile>> = HashMap::new();
    for r in file_rows {
        files.entry(r.item_id).or_default().push(r.into());
    }

    let status_rows: Vec<(i64, String, String, Option<String>, i64, i64)> = sqlx::query_as(
        "SELECT st.item_id, st.source, st.status, st.detail, st.updated_at, st.expires_at
         FROM item_status st JOIN item e ON e.id = st.item_id
         WHERE e.parent_id = ? ORDER BY st.updated_at DESC",
    )
    .bind(series_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut statuses: HashMap<i64, Vec<ExternalStatus>> = HashMap::new();
    for (item, source, status, detail, updated_at, expires_at) in status_rows {
        statuses.entry(item).or_default().push(ExternalStatus {
            source,
            status,
            detail: detail.and_then(|d| serde_json::from_str(&d).ok()),
            updated_at,
            expires_at,
            stale: expires_at <= now,
        });
    }

    let rows: Vec<&EpisodeRow> = resolved
        .entries
        .iter()
        .map(|(id, _)| &loaded.episodes[id])
        .collect();
    let availability: Vec<Availability> = rows.iter().map(|e| loaded.availability(e)).collect();
    let gaps = rules::gaps(&availability, &counts_for_gaps(&resolved.entries));
    let played: Vec<(Availability, bool)> = rows
        .iter()
        .zip(&availability)
        .map(|(e, a)| (*a, Loaded::played(e)))
        .collect();
    let up_next = rules::up_next(&played);

    let entries = resolved
        .entries
        .iter()
        .zip(rows.iter().zip(availability.iter().zip(&gaps)))
        .map(|((_, number), (e, (a, gap)))| {
            let monitored = loaded.monitored(e);
            Episode {
                episode: loaded.episode_ref(e, number.clone()),
                availability: *a,
                monitored,
                wanted: e.wanted_reason.is_some(),
                wanted_reason: e.wanted_reason.clone(),
                overview: e.overview.clone(),
                is_filler: e.is_filler,
                is_gap: *gap,
                watch: e.position_seconds.map(|position_seconds| WatchState {
                    position_seconds,
                    played: Loaded::played(e),
                    last_played_at: e.last_played_at,
                }),
                files: files.remove(&e.id).unwrap_or_default(),
                statuses: statuses.get(&e.id).cloned().unwrap_or_default(),
            }
        })
        .collect();

    // Count each episode once, even if the view lists it twice.
    let mut seen = HashSet::new();
    let unique = rows.iter().filter(|e| seen.insert(e.id)).copied();
    Ok(EpisodeList {
        view: resolved.view.map(|v| v.id),
        counts: loaded.counts(unique),
        gaps: gaps.iter().filter(|g| **g).count() as i64,
        up_next,
        entries,
    })
}

#[derive(sqlx::FromRow)]
pub(crate) struct FileRow {
    item_id: i64,
    file_id: i64,
    root_path: String,
    relative_path: String,
    status: String,
    size: i64,
    last_seen: i64,
    container: Option<String>,
    video_codec: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    hdr_format: Option<String>,
    edition: Option<String>,
    part: Option<i64>,
    link_source: String,
    confidence: f64,
    pinned: bool,
}

impl From<FileRow> for LinkedFile {
    fn from(r: FileRow) -> Self {
        LinkedFile {
            file_id: r.file_id,
            root_path: r.root_path,
            relative_path: r.relative_path,
            status: r.status,
            size: r.size,
            last_seen: r.last_seen,
            container: r.container,
            video_codec: r.video_codec,
            width: r.width,
            height: r.height,
            hdr_format: r.hdr_format,
            edition: r.edition,
            part: r.part,
            link_source: r.link_source,
            confidence: r.confidence,
            pinned: r.pinned,
        }
    }
}

pub async fn home(pool: &SqlitePool, user: Option<i64>, now: i64) -> Result<Home, CatalogError> {
    let mut conn = pool.acquire().await?;
    let mut shows = Vec::new();
    let mut keep_watching = Vec::new();
    let mut coming_up = Vec::new();
    let mut attention = Attention::default();

    for id in series_ids(&mut conn, None).await? {
        let loaded = Loaded::load(&mut conn, id, user, now).await?;
        let views = views(&mut conn, id, user).await?;
        let pref = user_view(&mut conn, id, user).await?;
        let resolved = resolve(&mut conn, &loaded, &views, pref).await?;
        let numbers = resolved.numbers();
        let summary = loaded.summary(&numbers);
        attention.wanted += summary.counts.wanted;

        let rows: Vec<&EpisodeRow> = resolved
            .entries
            .iter()
            .map(|(id, _)| &loaded.episodes[id])
            .collect();
        let played: Vec<(Availability, bool)> = rows
            .iter()
            .map(|e| (loaded.availability(e), Loaded::played(e)))
            .collect();
        let last_played = loaded
            .episodes
            .values()
            .filter_map(|e| e.last_played_at)
            .max();
        if let (Some(last_played_at), Some(i)) = (last_played, rules::up_next(&played)) {
            let e = rows[i];
            keep_watching.push(ContinueItem {
                series_id: id,
                series_title: loaded.series.title.clone(),
                view_name: resolved.view.as_ref().map(|v| v.name.clone()),
                episode: loaded.episode_ref(e, resolved.entries[i].1.clone()),
                position_seconds: e.position_seconds.unwrap_or(0.0),
                last_played_at,
            });
        }

        for e in loaded.episodes.values() {
            if e.airs_at
                .is_some_and(|t| t > now && t <= now + COMING_UP_SECONDS)
            {
                coming_up.push(ComingUp {
                    series_id: id,
                    series_title: loaded.series.title.clone(),
                    episode: loaded.episode_ref(e, numbers.get(&e.id).cloned()),
                });
            }
        }

        // Gaps are counted in the default ordering, the same for every user.
        let default = resolve(&mut conn, &loaded, &views, None).await?;
        let availability: Vec<Availability> = default
            .entries
            .iter()
            .map(|(id, _)| loaded.availability(&loaded.episodes[id]))
            .collect();
        let gaps = rules::gaps(&availability, &counts_for_gaps(&default.entries))
            .iter()
            .filter(|g| **g)
            .count() as i64;
        attention.gaps += gaps;
        attention.gap_series += i64::from(gaps > 0);

        shows.push(summary);
    }

    keep_watching.sort_by_key(|c| std::cmp::Reverse(c.last_played_at));
    coming_up.sort_by_key(|c| (c.episode.airs_at, c.episode.item_id));

    attention.review =
        sqlx::query_scalar("SELECT count(*) FROM match_candidate WHERE state = 'pending'")
            .fetch_one(&mut *conn)
            .await?;
    attention.missing_files =
        sqlx::query_scalar("SELECT count(*) FROM media_file WHERE status = 'missing'")
            .fetch_one(&mut *conn)
            .await?;
    let roots: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT r.path FROM library_root r
         JOIN media_file f ON f.root_id = r.id WHERE f.status = 'missing' ORDER BY r.path",
    )
    .fetch_all(&mut *conn)
    .await?;
    for root in roots {
        if !tokio::fs::metadata(&root).await.is_ok_and(|m| m.is_dir()) {
            attention.offline_roots.push(root);
        }
    }
    let query = sqlx::query_as::<_, (i64, Option<String>)>(
        "SELECT count(DISTINCT item_id), group_concat(DISTINCT source) FROM item_status
         WHERE expires_at > ? AND status IN (SELECT value FROM json_each(?))",
    )
    .bind(now)
    .bind(json!(IN_PROGRESS_STATUSES).to_string());
    let (in_progress, sources) = query.fetch_one(&mut *conn).await?;
    attention.in_progress = in_progress;
    attention.status_sources = sources
        .map(|s| s.split(',').map(String::from).collect())
        .unwrap_or_default();
    attention.status_sources.sort();

    drop(conn);
    let movies = movies::list_movies(pool, user, None).await?;
    Ok(Home {
        movies,
        hero: keep_watching.first().cloned(),
        keep_watching,
        coming_up,
        shows,
        attention,
    })
}

// ── Changes ────────────────────────────────────────────────────────────────

/// Sets which ordering or playlist drives the user's "next up" for a series.
pub async fn set_view(
    pool: &SqlitePool,
    series_id: i64,
    user: i64,
    view: ViewRef,
) -> Result<(), CatalogError> {
    let mut conn = pool.acquire().await?;
    let views = views(&mut conn, series_id, Some(user)).await?;
    if views.is_empty()
        && Loaded::load(&mut conn, series_id, Some(user), now())
            .await
            .is_err()
    {
        return Err(CatalogError::NotFound);
    }
    if !views.iter().any(|v| v.id == view.key()) {
        return Err(CatalogError::InvalidView(view.key()));
    }
    let (ordering, playlist) = match view {
        ViewRef::Ordering(id) => (Some(id), None),
        ViewRef::Playlist(id) => (None, Some(id)),
    };
    sqlx::query(
        "INSERT INTO user_series_pref (user_id, series_id, ordering_id, playlist_id)
         VALUES (?, ?, ?, ?)
         ON CONFLICT (user_id, series_id) DO UPDATE SET
             ordering_id = excluded.ordering_id, playlist_id = excluded.playlist_id",
    )
    .bind(user)
    .bind(series_id)
    .bind(ordering)
    .bind(playlist)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Sets an item's own monitored flag (`None` inherits from the parent), and emits
/// `item.wanted` / `item.unwanted` for every item whose wanted state changes.
pub async fn set_monitored(
    pool: &SqlitePool,
    item_id: i64,
    monitored: Option<bool>,
) -> Result<(), CatalogError> {
    let mut tx = crate::db::begin_write(pool).await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM item WHERE id = ?)")
        .bind(item_id)
        .fetch_one(&mut *tx)
        .await?;
    if !exists {
        return Err(CatalogError::NotFound);
    }
    sqlx::query(
        "INSERT INTO monitor (item_id, monitored) VALUES (?, ?)
         ON CONFLICT (item_id) DO UPDATE SET monitored = excluded.monitored",
    )
    .bind(item_id)
    .bind(monitored)
    .execute(&mut *tx)
    .await?;
    crate::wanted::sync_events(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Where an episode or movie sits, for the player.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlayContext {
    pub item_id: i64,
    /// `episode` or `movie`.
    pub kind: String,
    pub title: String,
    pub year: Option<i64>,
    pub series_id: Option<i64>,
    pub series_title: Option<String>,
    /// The episode's number in the user's ordering or playlist.
    pub number: Option<EpisodeNumber>,
    /// The next available episode there.
    pub next: Option<EpisodeRef>,
}

pub async fn play_context(
    pool: &SqlitePool,
    item: i64,
    user: Option<i64>,
    now: i64,
) -> Result<PlayContext, CatalogError> {
    let (kind, title, year, parent): (String, String, Option<i64>, Option<i64>) =
        sqlx::query_as("SELECT kind, title, year, parent_id FROM item WHERE id = ?")
            .bind(item)
            .fetch_optional(pool)
            .await?
            .ok_or(CatalogError::NotFound)?;
    let mut context = PlayContext {
        item_id: item,
        kind,
        title,
        year,
        series_id: parent,
        series_title: None,
        number: None,
        next: None,
    };
    let Some(series) = parent else {
        return Ok(context);
    };
    context.series_title = sqlx::query_scalar("SELECT title FROM item WHERE id = ?")
        .bind(series)
        .fetch_optional(pool)
        .await?;
    let list = episodes(pool, series, None, user, now).await?;
    if let Some(at) = list.entries.iter().position(|e| e.episode.item_id == item) {
        context.number = list.entries[at].episode.number.clone();
        context.next = list.entries[at + 1..]
            .iter()
            .find(|e| e.availability == Availability::Available)
            .map(|e| e.episode.clone());
    }
    Ok(context)
}
