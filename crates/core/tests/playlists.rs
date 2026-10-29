//! Playlist CRUD, visibility, and what the catalog makes of them.

use cabinet_core::catalog::{self, ViewRef};
use cabinet_core::playlists::{self, Actor, PlaylistError, PlaylistInput};
use sqlx::SqlitePool;
use tempfile::TempDir;

const ADMIN: Actor = Actor {
    user: 1,
    admin: true,
};
const KID: Actor = Actor {
    user: 3,
    admin: false,
};

async fn demo() -> (TempDir, SqlitePool, i64) {
    let dir = tempfile::tempdir().unwrap();
    let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
        .await
        .unwrap();
    cabinet_core::demo::seed(&pool, &dir.path().join("media"), catalog::now())
        .await
        .unwrap();
    sqlx::query("INSERT INTO app_user (id, name) VALUES (3, 'kid')")
        .execute(&pool)
        .await
        .unwrap();
    let kitsune = sqlx::query_scalar("SELECT id FROM item WHERE title = 'Kitsune Road'")
        .fetch_one(&pool)
        .await
        .unwrap();
    (dir, pool, kitsune)
}

async fn episodes(pool: &SqlitePool, series: i64) -> Vec<i64> {
    sqlx::query_scalar("SELECT id FROM item WHERE parent_id = ? ORDER BY id")
        .bind(series)
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn manual_playlists_are_private_and_ordered() {
    let (_dir, pool, kitsune) = demo().await;
    let eps = episodes(&pool, kitsune).await;
    let picks = vec![eps[3], eps[0], eps[1]];
    let mine = playlists::create(
        &pool,
        KID,
        PlaylistInput {
            name: Some("  Best bits ".into()),
            kind: Some("manual".into()),
            items: Some(picks.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        (mine.name.as_str(), mine.series_id, mine.shared),
        ("Best bits", kitsune, false)
    );
    assert_eq!(mine.items, picks);
    assert!(
        playlists::get(&pool, ADMIN.user, mine.id).await.is_err(),
        "admins don't see it either"
    );

    // The catalog lists it for its owner and follows it.
    catalog::set_view(&pool, kitsune, KID.user, ViewRef::Playlist(mine.id))
        .await
        .unwrap();
    let list = catalog::episodes(&pool, kitsune, None, Some(KID.user), catalog::now())
        .await
        .unwrap();
    assert_eq!(
        list.entries
            .iter()
            .map(|e| e.episode.item_id)
            .collect::<Vec<_>>(),
        picks
    );

    let updated = playlists::update(
        &pool,
        KID,
        mine.id,
        PlaylistInput {
            items: Some(vec![eps[2]]),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.items, [eps[2]]);

    let other: i64 =
        sqlx::query_scalar("SELECT id FROM item WHERE kind = 'episode' AND parent_id != ? LIMIT 1")
            .bind(kitsune)
            .fetch_one(&pool)
            .await
            .unwrap();
    let mixed = PlaylistInput {
        name: Some("x".into()),
        kind: Some("manual".into()),
        items: Some(vec![eps[0], other]),
        ..Default::default()
    };
    assert!(matches!(
        playlists::create(&pool, KID, mixed).await,
        Err(PlaylistError::Invalid(_))
    ));
    playlists::delete(&pool, KID, mine.id).await.unwrap();
}

#[tokio::test]
async fn shared_playlists_need_an_admin() {
    let (_dir, pool, kitsune) = demo().await;
    let shared = playlists::list(&pool, KID.user, Some(kitsune))
        .await
        .unwrap();
    assert_eq!(shared.len(), 1);
    let no_filler = &shared[0];
    assert_eq!((no_filler.kind.as_str(), no_filler.shared), ("smart", true));
    assert_eq!(no_filler.exclude_tags, ["filler"]);

    let rename = || PlaylistInput {
        name: Some("Story only".into()),
        ..Default::default()
    };
    assert!(matches!(
        playlists::update(&pool, KID, no_filler.id, rename()).await,
        Err(PlaylistError::Forbidden)
    ));
    let create_shared = PlaylistInput {
        name: Some("Ours".into()),
        kind: Some("smart".into()),
        shared: true,
        base_ordering_id: no_filler.base_ordering_id,
        ..Default::default()
    };
    assert!(matches!(
        playlists::create(&pool, KID, create_shared).await,
        Err(PlaylistError::Forbidden)
    ));

    // Wanted follows the playlist, so changing its rules changes what's wanted.
    cabinet_core::wanted::set_monitoring(&pool, kitsune, None, Some(Some(no_filler.id)))
        .await
        .unwrap();
    let wanted = || async {
        cabinet_core::wanted::list(
            &pool,
            &cabinet_core::wanted::WantedFilter {
                series_id: Some(kitsune),
                ..Default::default()
            },
            catalog::now(),
        )
        .await
        .unwrap()
        .total
    };
    assert_eq!(wanted().await, 2);
    playlists::update(
        &pool,
        ADMIN,
        no_filler.id,
        PlaylistInput {
            exclude_tags: Some(vec![]),
            ..rename()
        },
    )
    .await
    .unwrap();
    assert_eq!(wanted().await, 4);
    let became: i64 = sqlx::query_scalar("SELECT count(*) FROM wanted_snapshot WHERE item_id IN (SELECT id FROM item WHERE parent_id = ?)")
        .bind(kitsune)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(became, 4, "events follow straight away");
}
