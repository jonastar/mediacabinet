//! Demo data matching the design mockups, for developing the UI.
//!
//! Kitsune Road is the mockup's Series page exactly: aired and absolute orderings,
//! a no-filler playlist, filler tags, versions, a file on an offline root, an
//! external download status and watch progress. Seven more shows fill Home. Dates
//! are relative to now, so "coming this week" always has something in it.
//!
//! Roots are created under `media_dir`; the "old NAS" root is deliberately not, so
//! it shows as offline. Never scan these roots: the files only exist in the database.

use std::path::Path;

use serde_json::json;
use sqlx::{SqliteConnection, SqlitePool};

const DAY: i64 = 24 * 3600;
const GB: i64 = 1_000_000_000;

#[derive(Debug, thiserror::Error)]
pub enum DemoError {
    #[error(
        "the database already has libraries; seed an empty one (e.g. CABINET_DATA_DIR=./data-demo)"
    )]
    NotEmpty,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

pub async fn seed(pool: &SqlitePool, media_dir: &Path, now: i64) -> Result<(), DemoError> {
    let libraries: i64 = sqlx::query_scalar("SELECT count(*) FROM library")
        .fetch_one(pool)
        .await?;
    if libraries > 0 {
        return Err(DemoError::NotEmpty);
    }
    let tv_root = media_dir.join("tv");
    std::fs::create_dir_all(&tv_root)?;
    let old_root = media_dir.join("old-nas-offline");

    let mut tx = crate::db::begin_write(pool).await?;
    let mut s = Seeder { conn: &mut tx, now };
    s.exec("INSERT INTO app_user (id, name, is_admin) VALUES (1, 'demo', 1)")
        .await?;
    // Quality profiles come from migration 0004; the demo uses HD 1080p.
    s.exec("INSERT INTO library (id, name, kind) VALUES (1, 'TV Shows', 'tv')")
        .await?;
    let template = "{series}/Season {season:00}/{filename}";
    let tv = s.root(&tv_root, template, "adaptive", true).await?;
    let old = s.root(&old_root, template, "read_only", false).await?;

    kitsune_road(&mut s, tv, old).await?;
    for show in OTHER_SHOWS {
        other_show(&mut s, tv, show).await?;
    }
    people(&mut tx).await?;
    // Start the wanted snapshot from the seeded state, as a real library would be
    // after its first scans.
    crate::wanted::sync_events(&mut tx).await?;
    activity(&mut tx, now).await?;
    tx.commit().await?;
    Ok(())
}

/// Invented cast and crew, networks and ratings for Kitsune Road and Harbor
/// Lights. People have no photos, so their initials show.
async fn people(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    use crate::metadata::import::{set_companies, set_credits, set_rating};
    use crate::metadata::{CompanyData, CreditData, Credits, PersonRef, RatingData};

    fn credit(id: &str, name: &str, role: &str, episodes: Option<i64>) -> CreditData {
        CreditData {
            person: PersonRef {
                id: format!("demo-{id}"),
                name: name.into(),
                photo: None,
                known_for: None,
            },
            role: Some(role.into()),
            department: None,
            episodes,
        }
    }
    fn crew(id: &str, name: &str, job: &str, department: &str) -> CreditData {
        CreditData {
            department: Some(department.into()),
            ..credit(id, name, job, None)
        }
    }
    let series = |title: &'static str| {
        sqlx::query_scalar::<_, i64>("SELECT id FROM item WHERE kind = 'series' AND title = ?")
            .bind(title)
    };
    let rate = |value: f64, scale: f64, votes: Option<i64>| RatingData {
        value,
        scale,
        votes,
    };

    let kitsune = series("Kitsune Road").fetch_one(&mut *conn).await?;
    let cast = vec![
        credit("1", "Haruka Mori", "Kon (voice)", Some(12)),
        credit("2", "Daichi Arai", "Old Tanuki (voice)", Some(12)),
        credit("3", "Mei Takeda", "Sayo (voice)", Some(9)),
        credit("4", "Ren Kuroda", "The Pilgrim (voice)", Some(6)),
        credit("5", "Aoi Fujimura", "Shrine Keeper (voice)", Some(4)),
    ];
    let credits = Credits {
        cast,
        guests: vec![],
        crew: vec![crew("10", "Sora Ishikawa", "Creator", "Writing")],
    };
    set_credits(conn, kitsune, "demo", &credits).await?;
    set_companies(
        conn,
        kitsune,
        "demo",
        "network",
        &[CompanyData {
            id: None,
            name: "Tokyo MX".into(),
        }],
    )
    .await?;
    set_companies(
        conn,
        kitsune,
        "demo",
        "studio",
        &[CompanyData {
            id: None,
            name: "Studio Hokora".into(),
        }],
    )
    .await?;
    for (source, r) in [
        ("tmdb", rate(8.3, 10.0, Some(214))),
        ("imdb", rate(8.1, 10.0, Some(5_412))),
        ("rotten_tomatoes", rate(100.0, 100.0, None)),
    ] {
        set_rating(conn, kitsune, source, Some(r)).await?;
    }
    sqlx::query("UPDATE item SET content_rating = 'TV-PG', tagline = 'Every debt has a road home.' WHERE id = ?")
        .bind(kitsune)
        .execute(&mut *conn)
        .await?;
    let episodes: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM item WHERE parent_id = ? ORDER BY airs_at IS NULL, airs_at LIMIT 4",
    )
    .bind(kitsune)
    .fetch_all(&mut *conn)
    .await?;
    let directors = [
        ("11", "Yuki Hamada"),
        ("12", "Kenta Oshiro"),
        ("11", "Yuki Hamada"),
        ("13", "Nanami Sato"),
    ];
    let guests = [
        Some(("20", "Takumi Endo", "Ferryman (voice)")),
        Some(("21", "Rina Kasai", "Lantern Seller (voice)")),
        None,
        Some(("20", "Takumi Endo", "Ferryman (voice)")),
    ];
    for ((episode, (id, name)), guest) in episodes.iter().zip(directors).zip(guests) {
        let credits = Credits {
            cast: vec![],
            guests: guest
                .map(|(id, name, role)| vec![credit(id, name, role, None)])
                .unwrap_or_default(),
            crew: vec![
                crew(id, name, "Director", "Directing"),
                crew("10", "Sora Ishikawa", "Writer", "Writing"),
            ],
        };
        set_credits(conn, *episode, "demo", &credits).await?;
    }

    let harbor = series("Harbor Lights").fetch_one(&mut *conn).await?;
    let credits = Credits {
        cast: vec![
            credit("30", "Nora Whitfield", "Ines Calder", Some(18)),
            credit("31", "Tomás Okafor", "DI Rafe Moreno", Some(18)),
            // Also in Kitsune Road, so her page lists both.
            credit("3", "Mei Takeda", "Lin", Some(7)),
        ],
        guests: vec![],
        crew: vec![
            crew("32", "Hal Brennan", "Creator", "Writing"),
            crew("33", "Edie Marsh", "Creator", "Writing"),
        ],
    };
    set_credits(conn, harbor, "demo", &credits).await?;
    set_companies(
        conn,
        harbor,
        "demo",
        "network",
        &[CompanyData {
            id: None,
            name: "Channel 4".into(),
        }],
    )
    .await?;
    for (source, r) in [
        ("imdb", rate(7.6, 10.0, Some(18_330))),
        ("rotten_tomatoes", rate(88.0, 100.0, None)),
        ("metacritic", rate(71.0, 100.0, None)),
    ] {
        set_rating(conn, harbor, source, Some(r)).await?;
    }
    sqlx::query("UPDATE item SET content_rating = 'TV-14' WHERE id = ?")
        .bind(harbor)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Activity as the seeded state would have made it: the latest episodes of each
/// show added, what the demo user watched, and a second user, sam, who watched
/// some of the same.
async fn activity(conn: &mut SqliteConnection, now: i64) -> Result<(), sqlx::Error> {
    let sam: i64 =
        sqlx::query_scalar("INSERT INTO app_user (name, is_admin) VALUES ('sam', 0) RETURNING id")
            .fetch_one(&mut *conn)
            .await?;
    sqlx::query(
        "INSERT INTO activity (kind, at, library_id, item_id, audience, payload)
         SELECT 'items_added', ?1 - (series_id % 9) * 86400 - (series_id % 5) * 3600, library_id,
                series_id, 'everyone', json_object('items', json_group_array(id), 'new_series', json('false'))
         FROM (SELECT e.id, e.parent_id AS series_id, e.library_id,
                      row_number() OVER (PARTITION BY e.parent_id ORDER BY e.airs_at DESC) AS latest
               FROM item e WHERE e.kind = 'episode'
               AND EXISTS (SELECT 1 FROM file_link l WHERE l.item_id = e.id))
         WHERE latest <= 3
         GROUP BY series_id",
    )
    .bind(now)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "INSERT INTO activity (kind, at, actor_user_id, subject_user_id, library_id, item_id,
                               audience, payload)
         SELECT CASE WHEN st.played THEN 'watched' ELSE 'stopped_watching' END,
                st.last_played_at - (u.id = ?1) * 5 * 3600, u.id, u.id, i.library_id, i.id, 'everyone',
                json_object('position_start', 0.0,
                            'position_end', CASE WHEN st.played THEN i.runtime_seconds
                                                 ELSE st.position_seconds END,
                            'duration_seconds', i.runtime_seconds)
         FROM user_item_state st JOIN item i ON i.id = st.item_id
         JOIN app_user u ON u.id = 1 OR (u.id = ?1 AND st.played AND st.item_id % 3 = 0)
         WHERE st.user_id = 1 AND st.last_played_at IS NOT NULL",
    )
    .bind(sam)
    .execute(conn)
    .await
    .map(|_| ())
}

struct Seeder<'a> {
    conn: &'a mut SqliteConnection,
    now: i64,
}

impl Seeder<'_> {
    async fn exec(&mut self, sql: &'static str) -> Result<(), sqlx::Error> {
        sqlx::raw_sql(sql)
            .execute(&mut *self.conn)
            .await
            .map(|_| ())
    }

    async fn root(
        &mut self,
        path: &Path,
        template: &str,
        policy: &str,
        import_target: bool,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            "INSERT INTO library_root (library_id, path, write_policy, layout_template, is_import_target)
             VALUES (1, ?, ?, ?, ?) RETURNING id",
        )
        .bind(path.to_string_lossy().into_owned())
        .bind(policy)
        .bind(template)
        .bind(import_target)
        .fetch_one(&mut *self.conn)
        .await
    }

    async fn series(
        &mut self,
        title: &str,
        year: i64,
        status: &str,
        overview: &str,
        tmdb: &str,
    ) -> Result<i64, sqlx::Error> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO item (kind, title, year, overview, primary_provider, library_id)
             VALUES ('series', ?, ?, ?, 'tmdb', 1) RETURNING id",
        )
        .bind(title)
        .bind(year)
        .bind(overview)
        .fetch_one(&mut *self.conn)
        .await?;
        sqlx::query("INSERT INTO series (item_id, status) VALUES (?, ?)")
            .bind(id)
            .bind(status)
            .execute(&mut *self.conn)
            .await?;
        self.external_id(id, "tmdb", tmdb, true).await?;
        sqlx::query(
            "INSERT INTO monitor (item_id, monitored, quality_profile_id)
             VALUES (?, 1, (SELECT id FROM quality_profile WHERE name = 'HD 1080p'))",
        )
        .bind(id)
        .execute(&mut *self.conn)
        .await?;
        Ok(id)
    }

    async fn external_id(
        &mut self,
        item: i64,
        provider: &str,
        value: &str,
        primary: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO external_id (item_id, library_id, kind, provider, value, is_primary)
             SELECT id, library_id, kind, ?2, ?3, ?4 FROM item WHERE id = ?1",
        )
        .bind(item)
        .bind(provider)
        .bind(value)
        .bind(primary)
        .execute(&mut *self.conn)
        .await
        .map(|_| ())
    }

    async fn episode(
        &mut self,
        series: i64,
        title: &str,
        airs_at: Option<i64>,
        runtime_minutes: i64,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            "INSERT INTO item (parent_id, library_id, kind, title, airs_at, airs_tz, runtime_seconds)
             SELECT ?1, library_id, 'episode', ?2, ?3, 'Asia/Tokyo', ?4 FROM item WHERE id = ?1
             RETURNING id",
        )
        .bind(series)
        .bind(title)
        .bind(airs_at)
        .bind(runtime_minutes * 60)
        .fetch_one(&mut *self.conn)
        .await
    }

    async fn ordering(
        &mut self,
        series: i64,
        name: &str,
        kind: &str,
        source: &str,
        default: bool,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            "INSERT INTO ordering (series_id, name, kind, source, is_default)
             VALUES (?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(series)
        .bind(name)
        .bind(kind)
        .bind(source)
        .bind(default)
        .fetch_one(&mut *self.conn)
        .await
    }

    async fn season(&mut self, ordering: i64, number: i64) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            "INSERT INTO ordering_season (ordering_id, number) VALUES (?, ?) RETURNING id",
        )
        .bind(ordering)
        .bind(number)
        .fetch_one(&mut *self.conn)
        .await
    }

    async fn entry(
        &mut self,
        ordering: i64,
        position: i64,
        item: i64,
        season: Option<i64>,
        number: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO ordering_entry (ordering_id, position, item_id, ordering_season_id, episode_number)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(ordering)
        .bind(position)
        .bind(item)
        .bind(season)
        .bind(number)
        .execute(&mut *self.conn)
        .await
        .map(|_| ())
    }

    /// A linked file with one video and one audio stream, as a probe would record it.
    #[allow(clippy::too_many_arguments)]
    async fn file(
        &mut self,
        item: i64,
        root: i64,
        path: &str,
        video: Video,
        size: i64,
        status: &str,
        link: (&str, f64, bool),
        first_seen: i64,
    ) -> Result<i64, sqlx::Error> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO media_file
                 (root_id, library_id, relative_path, size, mtime, content_hash, status, container,
                  duration_seconds, probed_at, first_seen, last_seen)
             SELECT ?1, library_id, ?2, ?3, ?4, ?5, ?6, 'matroska,webm', 1440, ?7, ?8, ?9
             FROM library_root WHERE id = ?1
             RETURNING id",
        )
        .bind(root)
        .bind(path)
        .bind(size)
        .bind(first_seen)
        .bind(format!("demo:{root}:{path}"))
        .bind(status)
        .bind(first_seen)
        .bind(first_seen)
        .bind(if status == "missing" {
            self.now - 3 * 3600
        } else {
            self.now
        })
        .fetch_one(&mut *self.conn)
        .await?;
        sqlx::query(
            "INSERT INTO media_stream (file_id, stream_index, kind, codec, width, height, hdr_format, is_default)
             VALUES (?, 0, 'video', ?, ?, ?, ?, 1)",
        )
        .bind(id)
        .bind(video.codec)
        .bind(video.width)
        .bind(video.height)
        .bind(video.hdr)
        .execute(&mut *self.conn)
        .await?;
        sqlx::query(
            "INSERT INTO media_stream (file_id, stream_index, kind, codec, language, channels, is_default)
             VALUES (?, 1, 'audio', 'aac', 'jpn', 2, 1)",
        )
        .bind(id)
        .execute(&mut *self.conn)
        .await?;
        let (source, confidence, pinned) = link;
        sqlx::query(
            "INSERT INTO file_link (file_id, item_id, library_id, confidence, source, pinned)
             SELECT ?1, ?2, library_id, ?3, ?4, ?5 FROM media_file WHERE id = ?1",
        )
        .bind(id)
        .bind(item)
        .bind(confidence)
        .bind(source)
        .bind(pinned)
        .execute(&mut *self.conn)
        .await?;
        Ok(id)
    }

    async fn watch(
        &mut self,
        item: i64,
        position_seconds: f64,
        played: bool,
        last_played_at: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO user_item_state (user_id, item_id, position_seconds, played, play_count, last_played_at)
             VALUES (1, ?, ?, ?, ?, ?)",
        )
        .bind(item)
        .bind(position_seconds)
        .bind(played)
        .bind(i64::from(played))
        .bind(last_played_at)
        .execute(&mut *self.conn)
        .await
        .map(|_| ())
    }
}

#[derive(Clone, Copy)]
struct Video {
    codec: &'static str,
    width: i64,
    height: i64,
    hdr: Option<&'static str>,
}

const HEVC_1080: Video = Video {
    codec: "hevc",
    width: 1920,
    height: 1080,
    hdr: None,
};
const H264_720: Video = Video {
    codec: "h264",
    width: 1280,
    height: 720,
    hdr: None,
};
const HEVC_2160_HDR: Video = Video {
    codec: "hevc",
    width: 3840,
    height: 2160,
    hdr: Some("hdr10"),
};

/// The mockup's Series page.
async fn kitsune_road(s: &mut Seeder<'_>, tv: i64, old: i64) -> Result<(), sqlx::Error> {
    let now = s.now;
    let series = s
        .series(
            "Kitsune Road",
            2019,
            "continuing",
            "A young fox spirit walks the old pilgrim road between mountain shrines, settling \
             debts left by the travellers who came before her.",
            "93405",
        )
        .await?;
    s.external_id(series, "tvdb", "361205", false).await?;
    s.external_id(series, "anidb", "14421", false).await?;
    s.external_id(series, "imdb", "tt9208876", false).await?;
    for (root, path, primary) in [
        (tv, "anime/Kitsune Road", true),
        (old, "Kitsune Road", false),
    ] {
        sqlx::query(
            "INSERT INTO series_location (series_item_id, root_id, library_id, path, is_primary)
             SELECT ?1, ?2, library_id, ?3, ?4 FROM library_root WHERE id = ?2",
        )
        .bind(series)
        .bind(root)
        .bind(path)
        .bind(primary)
        .execute(&mut *s.conn)
        .await?;
    }

    let aired = s.ordering(series, "Aired", "aired", "tmdb", true).await?;
    let absolute = s
        .ordering(series, "Absolute", "absolute", "tvdb", false)
        .await?;
    let seasons = [s.season(aired, 1).await?, s.season(aired, 2).await?];
    sqlx::query(
        "INSERT INTO playlist (name, kind, base_ordering_id, rules)
         VALUES ('No filler', 'smart', ?, '{\"exclude_tags\": [\"filler\"]}')",
    )
    .bind(absolute)
    .execute(&mut *s.conn)
    .await?;

    // Season 1 aired weekly from 4 Apr 2019, 17:30 JST.
    let s1 = 1_554_366_600;
    let week = 7 * DAY;
    let episodes: [(i64, i64, &str, Option<i64>); 12] = [
        (1, 1, "The Fox at the Crossing", Some(s1)),
        (1, 2, "Ash and Lanterns", Some(s1 + week)),
        (1, 3, "Market Day", Some(s1 + 2 * week)),
        (1, 4, "A Debt Remembered", Some(s1 + 3 * week)),
        (1, 5, "The Mountain Road", Some(s1 + 4 * week)),
        (1, 6, "Bathhouse Detour", Some(s1 + 5 * week)),
        (1, 7, "Two Tails", Some(s1 + 6 * week)),
        (1, 8, "Foxfire", Some(s1 + 7 * week)),
        (2, 1, "Return to the Shrine", Some(now - 16 * DAY)),
        (2, 2, "Paper Charms", Some(now - 9 * DAY)),
        (2, 3, "The Ninth Gate", Some(now + 5 * DAY)),
        (2, 4, "TBA", None),
    ];
    let mut ids = Vec::new();
    for (i, (season, number, title, airs_at)) in episodes.iter().enumerate() {
        let id = s.episode(series, title, *airs_at, 24).await?;
        let position = i as i64 + 1;
        s.entry(
            aired,
            position,
            id,
            Some(seasons[*season as usize - 1]),
            *number,
        )
        .await?;
        s.entry(absolute, position, id, None, position).await?;
        ids.push(id);
    }
    for filler in [ids[2], ids[5]] {
        sqlx::query(
            "INSERT INTO item_tag (item_id, tag, source) VALUES (?, 'filler', 'animefillerlist')",
        )
        .bind(filler)
        .execute(&mut *s.conn)
        .await?;
    }

    let added = now - 30 * DAY;
    let path = |s: i64, e: i64| {
        format!("anime/Kitsune Road/Season {s:02}/Kitsune Road - S{s:02}E{e:02}.mkv")
    };
    s.file(
        ids[0],
        tv,
        &path(1, 1),
        HEVC_1080,
        12 * GB / 10,
        "present",
        ("nfo", 1.0, true),
        added,
    )
    .await?;
    s.file(
        ids[1],
        tv,
        &path(1, 2),
        HEVC_1080,
        11 * GB / 10,
        "present",
        ("parser", 0.97, false),
        added,
    )
    .await?;
    s.file(
        ids[3],
        tv,
        &path(1, 4),
        HEVC_1080,
        13 * GB / 10,
        "present",
        ("parser", 0.96, false),
        added,
    )
    .await?;
    s.file(
        ids[4],
        old,
        "Kitsune Road/Kitsune Road - S01E05.mkv",
        HEVC_1080,
        12 * GB / 10,
        "missing",
        ("parser", 0.95, false),
        added,
    )
    .await?;
    s.file(
        ids[5],
        tv,
        &path(1, 6),
        H264_720,
        6 * GB / 10,
        "present",
        ("user", 1.0, true),
        added,
    )
    .await?;
    s.file(
        ids[6],
        tv,
        &path(1, 7),
        HEVC_1080,
        12 * GB / 10,
        "present",
        ("parser", 0.98, false),
        added,
    )
    .await?;
    s.file(
        ids[7],
        tv,
        "anime/Kitsune Road/Season 01/Kitsune Road - S01E08 - 2160p.mkv",
        HEVC_2160_HDR,
        41 * GB / 10,
        "present",
        ("nfo", 1.0, false),
        added,
    )
    .await?;
    s.file(
        ids[7],
        tv,
        &path(1, 8),
        HEVC_1080,
        13 * GB / 10,
        "present",
        ("nfo", 1.0, false),
        added,
    )
    .await?;
    s.file(
        ids[8],
        tv,
        &path(2, 1),
        HEVC_1080,
        14 * GB / 10,
        "present",
        ("parser", 0.99, false),
        now - 2 * DAY,
    )
    .await?;

    sqlx::query(
        "INSERT INTO item_status (item_id, source, status, detail, updated_at, expires_at)
         VALUES (?, 'sonarr', 'downloading', ?, ?, ?)",
    )
    .bind(ids[9])
    .bind(json!({ "progress": 0.4, "eta_seconds": 540 }).to_string())
    .bind(now - 120)
    .bind(now + 3600)
    .execute(&mut *s.conn)
    .await?;

    for i in [0, 1, 3] {
        s.watch(ids[i], 1440.0, true, now - (10 - i as i64) * DAY)
            .await?;
    }
    // 62% into Two Tails: 14 minutes left of 24... the mockup's numbers, rounded.
    s.watch(ids[6], 0.62 * 1440.0, false, now - 3600).await?;
    sqlx::query(
        "INSERT INTO user_series_pref (user_id, series_id, playlist_id)
         SELECT 1, ?, id FROM playlist WHERE name = 'No filler'",
    )
    .bind(series)
    .execute(&mut *s.conn)
    .await?;
    Ok(())
}

struct Show {
    title: &'static str,
    year: i64,
    status: &'static str,
    tmdb: &'static str,
    seasons: &'static [i64],
    /// Episodes from the start with a present file.
    have: i64,
    /// Of those, how many (from the end) are missing instead.
    missing: i64,
    /// The last N episodes air in the future: (days from now, hour UTC, minute).
    upcoming: &'static [(i64, i64, i64)],
    /// Episodes played from the start, then optionally one in progress (fraction).
    played: i64,
    in_progress: Option<f64>,
    /// Days since last watched; None if never.
    last_watched_days: Option<i64>,
    runtime: i64,
}

const OTHER_SHOWS: &[Show] = &[
    Show {
        title: "Harbor Lights",
        year: 2021,
        status: "continuing",
        tmdb: "99101",
        seasons: &[13, 13, 14],
        have: 38,
        missing: 0,
        upcoming: &[(1, 3, 0)],
        played: 27,
        in_progress: Some(0.28),
        last_watched_days: Some(1),
        runtime: 44,
    },
    Show {
        title: "Lowland",
        year: 2024,
        status: "continuing",
        tmdb: "99102",
        seasons: &[8, 8],
        have: 8,
        missing: 0,
        upcoming: &[
            (3, 22, 0),
            (10, 22, 0),
            (17, 22, 0),
            (24, 22, 0),
            (31, 22, 0),
            (38, 22, 0),
            (45, 22, 0),
            (52, 22, 0),
        ],
        played: 0,
        in_progress: None,
        last_watched_days: None,
        runtime: 50,
    },
    Show {
        title: "The Long Orbit",
        year: 2022,
        status: "ended",
        tmdb: "99103",
        seasons: &[10, 10],
        have: 20,
        missing: 0,
        upcoming: &[],
        played: 8,
        in_progress: None,
        last_watched_days: Some(3),
        runtime: 52,
    },
    Show {
        title: "Paper Saints",
        year: 2023,
        status: "continuing",
        tmdb: "99104",
        seasons: &[12, 12],
        have: 18,
        missing: 6,
        upcoming: &[],
        played: 3,
        in_progress: Some(0.81),
        last_watched_days: Some(2),
        runtime: 42,
    },
    Show {
        title: "Signal & Noise",
        year: 2018,
        status: "ended",
        tmdb: "99105",
        seasons: &[10, 10, 10],
        have: 30,
        missing: 0,
        upcoming: &[],
        played: 0,
        in_progress: None,
        last_watched_days: None,
        runtime: 45,
    },
    Show {
        title: "The Quiet Year",
        year: 2025,
        status: "continuing",
        tmdb: "99106",
        seasons: &[10],
        have: 4,
        missing: 0,
        upcoming: &[
            (6, 2, 15),
            (13, 2, 15),
            (20, 2, 15),
            (27, 2, 15),
            (34, 2, 15),
            (41, 2, 15),
        ],
        played: 0,
        in_progress: None,
        last_watched_days: None,
        runtime: 38,
    },
    Show {
        title: "Ferrymen",
        year: 2015,
        status: "ended",
        tmdb: "99107",
        seasons: &[22, 22, 22],
        have: 62,
        missing: 0,
        upcoming: &[],
        played: 0,
        in_progress: None,
        last_watched_days: None,
        runtime: 22,
    },
];

const TITLE_WORDS: &[&str] = &[
    "Low Tide",
    "The Breakwater",
    "Vellum",
    "Perihelion",
    "Floodplain",
    "Snowmelt",
    "Undertow",
    "Night Ferry",
    "Static",
    "Lantern",
    "The Long Way",
    "Saltwater",
    "Crosswind",
    "Ember",
    "Quiet Hours",
    "Driftwood",
    "The Signal",
    "Northbound",
    "Glasswork",
    "Paper Moon",
];

async fn other_show(s: &mut Seeder<'_>, tv: i64, show: &Show) -> Result<(), sqlx::Error> {
    let now = s.now;
    let series = s
        .series(
            show.title,
            show.year,
            show.status,
            "[Series overview from the primary provider.]",
            show.tmdb,
        )
        .await?;
    sqlx::query(
        "INSERT INTO series_location (series_item_id, root_id, library_id, path, is_primary)
         SELECT ?1, ?2, library_id, ?3, 1 FROM library_root WHERE id = ?2",
    )
    .bind(series)
    .bind(tv)
    .bind(show.title)
    .execute(&mut *s.conn)
    .await?;
    let aired = s.ordering(series, "Aired", "aired", "tmdb", true).await?;

    let total: i64 = show.seasons.iter().sum();
    let first_upcoming = total - show.upcoming.len() as i64;
    let today = now - now.rem_euclid(DAY);
    let mut position = 0;
    for (season_index, episodes) in show.seasons.iter().enumerate() {
        let season_number = season_index as i64 + 1;
        let season = s.season(aired, season_number).await?;
        for number in 1..=*episodes {
            let i = position;
            position += 1;
            let airs_at = if i >= first_upcoming {
                let (days, hour, minute) = show.upcoming[(i - first_upcoming) as usize];
                today + days * DAY + hour * 3600 + minute * 60
            } else {
                // Weekly, ending a week before the first upcoming one (or a month ago).
                now - (first_upcoming - i) * 7 * DAY
                    - 30 * DAY * i64::from(show.upcoming.is_empty())
            };
            let title = TITLE_WORDS[(i as usize + show.title.len()) % TITLE_WORDS.len()];
            let id = s
                .episode(series, title, Some(airs_at), show.runtime)
                .await?;
            s.entry(aired, position, id, Some(season), number).await?;

            if i < show.have {
                let status = if i >= show.have - show.missing {
                    "missing"
                } else {
                    "present"
                };
                let path = format!(
                    "{0}/Season {season_number:02}/{0} - S{season_number:02}E{number:02}.mkv",
                    show.title
                );
                let first_seen = if i == show.have - 1 && show.title == "Harbor Lights" {
                    now - DAY
                } else {
                    now - 60 * DAY
                };
                s.file(
                    id,
                    tv,
                    &path,
                    HEVC_1080,
                    2 * GB,
                    status,
                    ("parser", 0.97, false),
                    first_seen,
                )
                .await?;
            }
            if let Some(days) = show.last_watched_days {
                let last = now - days * DAY;
                if i < show.played {
                    s.watch(
                        id,
                        (show.runtime * 60) as f64,
                        true,
                        last - (show.played - i) * 3600,
                    )
                    .await?;
                } else if i == show.played
                    && let Some(fraction) = show.in_progress
                {
                    s.watch(id, fraction * (show.runtime * 60) as f64, false, last)
                        .await?;
                }
            }
        }
    }
    Ok(())
}
