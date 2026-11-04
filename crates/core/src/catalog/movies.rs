//! Movies, and searching the library.

use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};
use utoipa::ToSchema;

use super::rules::Availability;
use super::{CatalogError, EpisodeRef, ExternalId, LinkedFile, SeriesSummary, WatchState};

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MovieSummary {
    pub id: i64,
    /// The library it belongs to; null only if it predates having one.
    pub library_id: Option<i64>,
    pub title: String,
    pub year: Option<i64>,
    pub runtime_seconds: Option<i64>,
    /// Release, Unix seconds.
    pub release: Option<i64>,
    pub availability: Availability,
    pub monitored: bool,
    pub wanted: bool,
    /// Present files: versions and editions.
    pub versions: i64,
    pub watch: Option<WatchState>,
    pub genres: Vec<String>,
    pub artwork: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MovieDetail {
    #[serde(flatten)]
    pub summary: MovieSummary,
    pub original_title: Option<String>,
    pub overview: Option<String>,
    pub primary_provider: Option<String>,
    pub external_ids: Vec<ExternalId>,
    pub files: Vec<LinkedFile>,
    pub locked_fields: Vec<String>,
    pub refreshed_at: Option<i64>,
    pub refresh_error: Option<String>,
}

#[derive(sqlx::FromRow)]
struct MovieExtra {
    original_title: Option<String>,
    overview: Option<String>,
    primary_provider: Option<String>,
    metadata_locked: String,
    refreshed_at: Option<i64>,
    refresh_error: Option<String>,
}

#[derive(sqlx::FromRow)]
struct EpisodeHitRow {
    series_id: i64,
    series_title: String,
    id: i64,
    title: String,
    airs_at: Option<i64>,
    airs_tz: Option<String>,
    runtime_seconds: Option<i64>,
    season: Option<i64>,
    number: Option<i64>,
}

#[derive(sqlx::FromRow)]
pub(super) struct MovieRow {
    id: i64,
    library_id: Option<i64>,
    title: String,
    year: Option<i64>,
    runtime_seconds: Option<i64>,
    airs_at: Option<i64>,
    present: i64,
    availability: String,
    monitored: bool,
    wanted_reason: Option<String>,
    position_seconds: Option<f64>,
    played: Option<bool>,
    last_played_at: Option<i64>,
}

pub(super) const MOVIE_SELECT: &str =
    "SELECT i.id, i.library_id, i.title, i.year, i.runtime_seconds, i.airs_at,
        (SELECT count(*) FROM file_link l JOIN media_file f ON f.id = l.file_id
         WHERE l.item_id = i.id AND f.status = 'present') AS present,
        st.availability, st.monitored, w.reason AS wanted_reason,
        s.position_seconds, s.played, s.last_played_at
     FROM item i
     JOIN item_state st ON st.item_id = i.id
     LEFT JOIN item_wanted w ON w.item_id = i.id
     LEFT JOIN user_item_state s ON s.item_id = i.id AND s.user_id = ?1
     WHERE i.kind = 'movie'";

pub(super) async fn summary(
    conn: &mut SqliteConnection,
    row: MovieRow,
) -> Result<MovieSummary, sqlx::Error> {
    let (genres, artwork) = super::genres_and_artwork(conn, row.id).await?;
    let availability = Availability::parse(&row.availability);
    let monitored = row.monitored;
    Ok(MovieSummary {
        id: row.id,
        library_id: row.library_id,
        title: row.title,
        year: row.year,
        runtime_seconds: row.runtime_seconds,
        release: row.airs_at,
        availability,
        monitored,
        wanted: row.wanted_reason.is_some(),
        versions: row.present,
        watch: row.position_seconds.map(|position_seconds| WatchState {
            position_seconds,
            played: row.played.unwrap_or(false),
            last_played_at: row.last_played_at,
        }),
        genres,
        artwork,
    })
}

/// Movies by title: those in `library`, or all of them. A folder library's videos
/// aren't movies to anyone browsing, so only asking for that library lists them.
pub async fn list_movies(
    pool: &SqlitePool,
    user: Option<i64>,
    library: Option<i64>,
) -> Result<Vec<MovieSummary>, CatalogError> {
    let mut conn = pool.acquire().await?;
    let rows: Vec<MovieRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{MOVIE_SELECT}
           AND (i.library_id = ?2
                OR (?2 IS NULL AND i.library_id IN (SELECT id FROM library WHERE kind = 'movies')))
         ORDER BY coalesce(i.sort_title, i.title) COLLATE NOCASE, i.id"
    )))
    .bind(user)
    .bind(library)
    .fetch_all(&mut *conn)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(summary(&mut conn, row).await?);
    }
    Ok(out)
}

pub async fn movie_detail(
    pool: &SqlitePool,
    id: i64,
    user: Option<i64>,
) -> Result<MovieDetail, CatalogError> {
    let mut conn = pool.acquire().await?;
    let row: MovieRow =
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{MOVIE_SELECT} AND i.id = ?2")))
            .bind(user)
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or(CatalogError::NotFound)?;
    let summary = summary(&mut conn, row).await?;
    let extra: MovieExtra = sqlx::query_as(
        "SELECT original_title, overview, primary_provider, metadata_locked, refreshed_at, refresh_error
         FROM item WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    let external_ids: Vec<(String, String, bool)> = sqlx::query_as(
        "SELECT provider, value, is_primary FROM external_id WHERE item_id = ? ORDER BY is_primary DESC, provider",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let files: Vec<super::FileRow> = sqlx::query_as(
        "SELECT l.item_id, f.id AS file_id, r.path AS root_path, f.relative_path, f.status,
                f.size, f.last_seen, f.container, v.codec AS video_codec, v.width, v.height,
                v.hdr_format, l.edition, l.part, l.source AS link_source, l.confidence, l.pinned
         FROM file_link l
         JOIN media_file f ON f.id = l.file_id
         JOIN library_root r ON r.id = f.root_id
         LEFT JOIN media_stream v ON v.file_id = f.id AND v.stream_index =
             (SELECT min(stream_index) FROM media_stream WHERE file_id = f.id AND kind = 'video')
         WHERE l.item_id = ? AND f.status <> 'removed'
         ORDER BY f.status = 'present' DESC, l.edition IS NOT NULL, v.height DESC, l.part, f.id",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(MovieDetail {
        summary,
        original_title: extra.original_title,
        overview: extra.overview,
        primary_provider: extra.primary_provider,
        external_ids: external_ids
            .into_iter()
            .map(|(provider, value, is_primary)| ExternalId {
                provider,
                value,
                is_primary,
            })
            .collect(),
        files: files.into_iter().map(Into::into).collect(),
        locked_fields: serde_json::from_str(&extra.metadata_locked).unwrap_or_default(),
        refreshed_at: extra.refreshed_at,
        refresh_error: extra.refresh_error,
    })
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EpisodeHit {
    pub series_id: i64,
    pub series_title: String,
    #[serde(flatten)]
    pub episode: EpisodeRef,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SearchResults {
    pub series: Vec<SeriesSummary>,
    pub movies: Vec<MovieSummary>,
    pub episodes: Vec<EpisodeHit>,
}

/// Library search by title, ignoring case. Series and movies first, then episodes.
pub async fn search(
    pool: &SqlitePool,
    query: &str,
    user: Option<i64>,
    now: i64,
) -> Result<SearchResults, CatalogError> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(SearchResults {
            series: Vec::new(),
            movies: Vec::new(),
            episodes: Vec::new(),
        });
    }
    let pattern = format!(
        "%{}%",
        q.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let series_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM item WHERE kind = 'series'
           AND (title LIKE ?1 ESCAPE '\\' OR original_title LIKE ?1 ESCAPE '\\')
         ORDER BY title COLLATE NOCASE LIMIT 20",
    )
    .bind(&pattern)
    .fetch_all(pool)
    .await?;
    let mut series = Vec::new();
    for id in series_ids {
        if let Ok(detail) = super::series_detail(pool, id, user, now).await {
            series.push(detail.summary);
        }
    }

    let mut conn = pool.acquire().await?;
    let rows: Vec<MovieRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{MOVIE_SELECT} AND (i.title LIKE ?2 ESCAPE '\\' OR i.original_title LIKE ?2 ESCAPE '\\')
         ORDER BY i.title COLLATE NOCASE LIMIT 20"
    )))
    .bind(user)
    .bind(&pattern)
    .fetch_all(&mut *conn)
    .await?;
    let mut movies = Vec::new();
    for row in rows {
        movies.push(summary(&mut conn, row).await?);
    }

    let hits: Vec<EpisodeHitRow> = sqlx::query_as(
            "SELECT s.id AS series_id, s.title AS series_title, e.id, e.title, e.airs_at, e.airs_tz,
                    e.runtime_seconds, os.number AS season, oe.episode_number AS number
             FROM item e JOIN item s ON s.id = e.parent_id
             LEFT JOIN ordering o ON o.series_id = s.id AND o.is_default
             LEFT JOIN ordering_entry oe ON oe.ordering_id = o.id AND oe.item_id = e.id
             LEFT JOIN ordering_season os ON os.id = oe.ordering_season_id
             WHERE e.kind = 'episode' AND e.title LIKE ?1 ESCAPE '\\' AND e.title NOT LIKE 'Episode %'
             GROUP BY e.id ORDER BY s.title COLLATE NOCASE, oe.position LIMIT 20",
        )
        .bind(&pattern)
        .fetch_all(&mut *conn)
        .await?;
    let episodes = hits
        .into_iter()
        .map(
            |EpisodeHitRow {
                 series_id,
                 series_title,
                 id,
                 title,
                 airs_at,
                 airs_tz,
                 runtime_seconds: runtime,
                 season,
                 number,
             }| {
                EpisodeHit {
                    series_id,
                    series_title,
                    episode: EpisodeRef {
                        item_id: id,
                        title,
                        number: number.map(|n| super::EpisodeNumber {
                            style: if season.is_some() {
                                super::NumberStyle::Season
                            } else {
                                super::NumberStyle::Absolute
                            },
                            season,
                            number: Some(n),
                        }),
                        airs_at,
                        airs_time_known: airs_tz.is_some(),
                        runtime_seconds: runtime,
                    },
                }
            },
        )
        .collect();
    Ok(SearchResults {
        series,
        movies,
        episodes,
    })
}
