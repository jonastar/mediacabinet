//! People: who is in a series, episode or movie, and what else in the library
//! someone is in. Also an item's other provider details: networks, studios,
//! ratings, tagline and age rating.

use std::collections::BTreeMap;

use serde::Serialize;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use super::{CatalogError, EpisodeNumber, NumberStyle};
use crate::metadata::{MetadataError, Provider};

/// Biographies are fetched again after this long.
const DETAILS_MAX_AGE: i64 = 90 * 24 * 3600;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Credit {
    pub person_id: i64,
    pub name: String,
    /// The photo is at `/api/people/{person_id}/photo`.
    pub has_photo: bool,
    /// The character for cast and guests, the job for crew.
    pub role: Option<String>,
    /// Crew only: Directing, Writing...
    pub department: Option<String>,
    /// A series' cast: how many episodes they're in.
    pub episodes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Rating {
    /// `imdb`, `rotten_tomatoes`, `metacritic` or `tmdb`.
    pub source: String,
    pub value: f64,
    /// What a perfect score is: 10 or 100.
    pub scale: f64,
    pub votes: Option<i64>,
}

/// What a series, episode or movie is beyond its title: people, companies and ratings.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct About {
    pub tagline: Option<String>,
    /// The age rating, e.g. PG-13 or TV-MA.
    pub content_rating: Option<String>,
    /// IMDb, Rotten Tomatoes, Metacritic, then TMDB; only those known.
    pub ratings: Vec<Rating>,
    pub networks: Vec<String>,
    pub studios: Vec<String>,
    /// A series' regulars or a movie's cast, in billing order.
    pub cast: Vec<Credit>,
    /// An episode's guest stars.
    pub guests: Vec<Credit>,
    /// Directors, writers, creators and other key crew; someone with two jobs is listed twice.
    pub crew: Vec<Credit>,
}

#[derive(sqlx::FromRow)]
struct CreditRow {
    kind: String,
    person_id: i64,
    name: String,
    has_photo: bool,
    role: Option<String>,
    department: Option<String>,
    episodes: Option<i64>,
}

pub async fn about(pool: &SqlitePool, item: i64) -> Result<About, CatalogError> {
    let row: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT tagline, content_rating FROM item WHERE id = ?")
            .bind(item)
            .fetch_optional(pool)
            .await?;
    let (tagline, content_rating) = row.ok_or(CatalogError::NotFound)?;
    let ratings: Vec<(String, f64, f64, Option<i64>)> = sqlx::query_as(
        "SELECT source, value, scale, votes FROM item_rating WHERE item_id = ?
         ORDER BY CASE source WHEN 'imdb' THEN 0 WHEN 'rotten_tomatoes' THEN 1
                              WHEN 'metacritic' THEN 2 ELSE 3 END, source",
    )
    .bind(item)
    .fetch_all(pool)
    .await?;
    let companies: Vec<(String, String)> = sqlx::query_as(
        "SELECT role, name FROM item_company WHERE item_id = ? ORDER BY role, position",
    )
    .bind(item)
    .fetch_all(pool)
    .await?;
    let credits: Vec<CreditRow> = sqlx::query_as(
        "SELECT c.kind, p.id AS person_id, p.name, p.photo IS NOT NULL AS has_photo, c.role,
                c.department, c.episodes
         FROM credit c JOIN person p ON p.id = c.person_id
         WHERE c.item_id = ? ORDER BY c.kind, c.position",
    )
    .bind(item)
    .fetch_all(pool)
    .await?;

    let company = |role: &str| {
        companies
            .iter()
            .filter(|(r, _)| r == role)
            .map(|(_, name)| name.clone())
            .collect()
    };
    let mut about = About {
        tagline,
        content_rating,
        ratings: ratings
            .into_iter()
            .map(|(source, value, scale, votes)| Rating {
                source,
                value,
                scale,
                votes,
            })
            .collect(),
        networks: company("network"),
        studios: company("studio"),
        cast: Vec::new(),
        guests: Vec::new(),
        crew: Vec::new(),
    };
    for c in credits {
        let list = match c.kind.as_str() {
            "cast" => &mut about.cast,
            "guest" => &mut about.guests,
            _ => &mut about.crew,
        };
        list.push(Credit {
            person_id: c.person_id,
            name: c.name,
            has_photo: c.has_photo,
            role: c.role,
            department: c.department,
            episodes: c.episodes,
        });
    }
    Ok(about)
}

/// An episode someone is credited on.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PersonEpisode {
    pub item_id: i64,
    pub title: String,
    /// In the series' default ordering.
    pub number: Option<EpisodeNumber>,
    pub role: Option<String>,
}

/// A series or movie in the library someone is in, with every role they have there.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PersonCredit {
    pub item_id: i64,
    /// `series` or `movie`.
    pub kind: String,
    pub library_id: i64,
    pub title: String,
    pub year: Option<i64>,
    /// Artwork kinds available from `/api/artwork/{id}/{kind}`.
    pub artwork: Vec<String>,
    /// Characters and jobs, most prominent first, each once.
    pub roles: Vec<String>,
    /// A series' regular: how many episodes they're in.
    pub episode_count: Option<i64>,
    /// Episodes they guest in or worked on, in order.
    pub episodes: Vec<PersonEpisode>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PersonDetail {
    pub id: i64,
    pub name: String,
    /// The photo is at `/api/people/{id}/photo`.
    pub has_photo: bool,
    pub known_for: Option<String>,
    pub biography: Option<String>,
    /// `YYYY-MM-DD`.
    pub birthday: Option<String>,
    pub deathday: Option<String>,
    pub birthplace: Option<String>,
    /// Newest first.
    pub credits: Vec<PersonCredit>,
}

#[derive(sqlx::FromRow)]
struct PersonRow {
    id: i64,
    name: String,
    has_photo: bool,
    known_for: Option<String>,
    biography: Option<String>,
    birthday: Option<String>,
    deathday: Option<String>,
    birthplace: Option<String>,
}

#[derive(sqlx::FromRow)]
struct PersonCreditRow {
    kind: String,
    role: Option<String>,
    episodes: Option<i64>,
    item_id: i64,
    item_kind: String,
    top_id: i64,
    title: String,
    season: Option<i64>,
    number: Option<i64>,
}

pub async fn person(pool: &SqlitePool, id: i64) -> Result<PersonDetail, CatalogError> {
    let p: PersonRow = sqlx::query_as(
        "SELECT id, name, photo IS NOT NULL AS has_photo, known_for, biography, birthday,
                deathday, birthplace
         FROM person WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(CatalogError::NotFound)?;

    let rows: Vec<PersonCreditRow> = sqlx::query_as(
        "SELECT c.kind, c.role, c.episodes, i.id AS item_id, i.kind AS item_kind,
                coalesce(i.parent_id, i.id) AS top_id, i.title,
                os.number AS season, oe.episode_number AS number
         FROM credit c
         JOIN item i ON i.id = c.item_id
         LEFT JOIN ordering o ON o.series_id = i.parent_id AND o.is_default
         LEFT JOIN ordering_entry oe ON oe.ordering_id = o.id AND oe.item_id = i.id
         LEFT JOIN ordering_season os ON os.id = oe.ordering_season_id
         WHERE c.person_id = ?
         GROUP BY c.item_id, c.kind, c.position
         ORDER BY c.kind <> 'cast', c.kind <> 'guest', oe.position, c.position",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;

    // Episodes are folded into their series: roles, a regular's episode count, episodes.
    type Folded = (Vec<String>, Option<i64>, Vec<PersonEpisode>);
    let mut by_top: BTreeMap<i64, Folded> = BTreeMap::new();
    for r in rows {
        let (roles, count, episodes) = by_top.entry(r.top_id).or_default();
        if let Some(role) = &r.role
            && !roles.contains(role)
        {
            roles.push(role.clone());
        }
        if r.kind == "cast" && r.item_kind != "episode" {
            *count = r.episodes;
        }
        if r.item_kind == "episode" {
            match episodes.iter_mut().find(|e| e.item_id == r.item_id) {
                // A guest star who also directed it.
                Some(e) => {
                    if let Some(role) = r.role {
                        e.role = Some(match e.role.take() {
                            Some(first) => format!("{first}, {role}"),
                            None => role,
                        });
                    }
                }
                None => episodes.push(PersonEpisode {
                    item_id: r.item_id,
                    title: r.title,
                    number: r.number.map(|n| EpisodeNumber {
                        style: if r.season.is_some() {
                            NumberStyle::Season
                        } else {
                            NumberStyle::Absolute
                        },
                        season: r.season,
                        number: Some(n),
                    }),
                    role: r.role,
                }),
            }
        }
    }

    let mut conn = pool.acquire().await?;
    let mut credits = Vec::with_capacity(by_top.len());
    for (top, (roles, episode_count, episodes)) in by_top {
        let (kind, library_id, title, year): (String, i64, String, Option<i64>) =
            sqlx::query_as("SELECT kind, library_id, title, year FROM item WHERE id = ?")
                .bind(top)
                .fetch_one(&mut *conn)
                .await?;
        let (_, artwork) = super::genres_and_artwork(&mut conn, top).await?;
        credits.push(PersonCredit {
            item_id: top,
            kind,
            library_id,
            title,
            year,
            artwork,
            roles,
            episode_count,
            episodes,
        });
    }
    credits.sort_by(|a, b| b.year.cmp(&a.year).then_with(|| a.title.cmp(&b.title)));

    Ok(PersonDetail {
        id: p.id,
        name: p.name,
        has_photo: p.has_photo,
        known_for: p.known_for,
        biography: p.biography,
        birthday: p.birthday,
        deathday: p.deathday,
        birthplace: p.birthplace,
        credits,
    })
}

/// Fetches someone's biography and dates from the provider, unless they were
/// fetched recently. A provider that can't be reached leaves what's there.
pub async fn fetch_details(
    pool: &SqlitePool,
    provider: &Provider,
    id: i64,
    now: i64,
) -> Result<(), CatalogError> {
    let row: Option<(String, String, Option<i64>)> =
        sqlx::query_as("SELECT provider, provider_id, details_fetched_at FROM person WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    let (source, provider_id, fetched) = row.ok_or(CatalogError::NotFound)?;
    if source != provider.name()
        || !provider.is_configured()
        || fetched.is_some_and(|at| now - at < DETAILS_MAX_AGE)
    {
        return Ok(());
    }
    match provider.person(&provider_id).await {
        Ok(d) => {
            sqlx::query(
                "UPDATE person SET name = ?, photo = coalesce(?, photo),
                     known_for = coalesce(?, known_for), biography = ?, birthday = ?,
                     deathday = ?, birthplace = ?, details_fetched_at = ?
                 WHERE id = ?",
            )
            .bind(d.name)
            .bind(d.photo)
            .bind(d.known_for)
            .bind(d.biography)
            .bind(d.birthday)
            .bind(d.deathday)
            .bind(d.birthplace)
            .bind(now)
            .bind(id)
            .execute(pool)
            .await?;
        }
        Err(MetadataError::NotFound) => {
            sqlx::query("UPDATE person SET details_fetched_at = ? WHERE id = ?")
                .bind(now)
                .bind(id)
                .execute(pool)
                .await?;
        }
        Err(e) => tracing::debug!(person = id, error = %e, "can't fetch person details"),
    }
    Ok(())
}
