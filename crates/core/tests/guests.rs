//! Guests: throwaway accounts for trying a demo server.

use cabinet_core::{accounts, guests};
use sqlx::SqlitePool;
use tempfile::TempDir;

async fn db() -> (TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
        .await
        .unwrap();
    accounts::setup(&pool, "jo", "long enough").await.unwrap();
    (dir, pool)
}

#[tokio::test]
async fn each_guest_is_their_own_and_goes_when_they_sign_out() {
    let (_dir, pool) = db().await;
    let (a, a_token) = guests::sign_in(&pool).await.unwrap();
    let (b, _) = guests::sign_in(&pool).await.unwrap();
    assert_ne!(a.id, b.id);
    for guest in [&a, &b] {
        assert!(guest.is_guest && guest.incognito && !guest.is_admin);
        assert!(guest.name.starts_with("guest-"));
    }
    let seen = accounts::session_user(&pool, &a_token).await.unwrap();
    assert_eq!(seen.map(|u| u.id), Some(a.id));

    // Guests aren't listed with everyone else, and can't sign in with a password.
    assert_eq!(accounts::list(&pool).await.unwrap().len(), 1);
    assert_eq!(guests::count(&pool).await.unwrap(), 2);
    assert!(accounts::login(&pool, &a.name, "").await.is_err());

    accounts::logout(&pool, &a_token).await.unwrap();
    assert!(accounts::user(&pool, a.id).await.is_err());
    assert_eq!(guests::count(&pool).await.unwrap(), 1);
}

#[tokio::test]
async fn a_guest_stays_while_a_device_is_signed_in() {
    let (_dir, pool) = db().await;
    let (guest, web) = guests::sign_in(&pool).await.unwrap();
    accounts::open_device_session(&pool, guest.id, "TV")
        .await
        .unwrap();
    accounts::logout(&pool, &web).await.unwrap();
    assert!(accounts::user(&pool, guest.id).await.is_ok());

    let (device_guest, _) = guests::sign_in_device(&pool, "Review iPad").await.unwrap();
    assert_eq!(
        accounts::devices(&pool, device_guest.id)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(guests::sign_in_device(&pool, " ").await.is_err());
}

#[tokio::test]
async fn expired_guests_are_swept_and_turning_guests_off_removes_them() {
    let (_dir, pool) = db().await;
    assert!(!guests::enabled(&pool, None).await.unwrap());
    assert!(guests::enabled(&pool, Some(true)).await.unwrap());
    guests::set_enabled(&pool, true).await.unwrap();
    assert!(guests::enabled(&pool, None).await.unwrap());

    let (old, _) = guests::sign_in(&pool).await.unwrap();
    sqlx::query("UPDATE session SET expires_at = unixepoch() - 1 WHERE user_id = ?")
        .bind(old.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(guests::sweep(&pool).await.unwrap(), 1);

    guests::sign_in(&pool).await.unwrap();
    guests::sign_in(&pool).await.unwrap();
    guests::set_enabled(&pool, false).await.unwrap();
    assert_eq!(guests::count(&pool).await.unwrap(), 0);
    assert_eq!(accounts::list(&pool).await.unwrap().len(), 1, "jo stays");
}

#[tokio::test]
async fn the_guest_seen_longest_ago_makes_room() {
    let (_dir, pool) = db().await;
    let mut ids = vec![];
    for _ in 0..guests::MAX_GUESTS {
        ids.push(guests::sign_in(&pool).await.unwrap().0.id);
    }
    // The first guest came back a minute ago, so the second has waited longest.
    sqlx::query("UPDATE session SET last_seen_at = last_seen_at + 60 WHERE user_id = ?")
        .bind(ids[0])
        .execute(&pool)
        .await
        .unwrap();
    guests::sign_in(&pool).await.unwrap();
    assert_eq!(guests::count(&pool).await.unwrap(), guests::MAX_GUESTS);
    assert!(accounts::user(&pool, ids[0]).await.is_ok());
    assert!(accounts::user(&pool, ids[1]).await.is_err());
}
