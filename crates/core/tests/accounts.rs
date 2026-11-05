//! Setup, logins, sessions and user management.

use cabinet_core::accounts::{self, AccountError};
use sqlx::SqlitePool;
use tempfile::TempDir;

async fn db() -> (TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
        .await
        .unwrap();
    (dir, pool)
}

#[tokio::test]
async fn setup_happens_once() {
    let (_dir, pool) = db().await;
    cabinet_core::catalog::ensure_default_user(&pool)
        .await
        .unwrap();
    assert!(accounts::setup_needed(&pool).await.unwrap());
    assert!(matches!(
        accounts::setup(&pool, "jo", "short").await,
        Err(AccountError::Invalid(_))
    ));
    let admin = accounts::setup(&pool, "jo", "long enough").await.unwrap();
    assert!(admin.is_admin);
    assert!(!accounts::setup_needed(&pool).await.unwrap());
    assert!(accounts::setup(&pool, "eve", "long enough").await.is_err());
    assert_eq!(
        accounts::list(&pool).await.unwrap().len(),
        1,
        "reuses the first user"
    );
}

#[tokio::test]
async fn logins_open_sessions() {
    let (_dir, pool) = db().await;
    accounts::setup(&pool, "Jo", "long enough").await.unwrap();
    assert!(matches!(
        accounts::login(&pool, "jo", "wrong password").await,
        Err(AccountError::BadLogin)
    ));
    assert!(matches!(
        accounts::login(&pool, "nobody", "long enough").await,
        Err(AccountError::BadLogin)
    ));
    let (user, token) = accounts::login(&pool, "jo", "long enough").await.unwrap();
    assert_eq!(user.name, "Jo", "names match without case");
    let stored: String = sqlx::query_scalar("SELECT token_hash FROM session")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(stored, token, "only the hash is stored");
    assert_eq!(
        accounts::session_user(&pool, &token)
            .await
            .unwrap()
            .map(|u| u.id),
        Some(user.id)
    );
    accounts::logout(&pool, &token).await.unwrap();
    assert!(
        accounts::session_user(&pool, &token)
            .await
            .unwrap()
            .is_none()
    );

    // Expired sessions don't count.
    let (_, token) = accounts::login(&pool, "jo", "long enough").await.unwrap();
    sqlx::query("UPDATE session SET expires_at = unixepoch() - 1")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        accounts::session_user(&pool, &token)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn managing_users() {
    let (_dir, pool) = db().await;
    let admin = accounts::setup(&pool, "jo", "long enough").await.unwrap();
    let kid = accounts::create(&pool, "kid", "kid password", false)
        .await
        .unwrap();
    assert!(
        accounts::create(&pool, "kid", "kid password", false)
            .await
            .is_err(),
        "taken"
    );
    assert!(
        accounts::update(&pool, admin.id, None, None, Some(false))
            .await
            .is_err(),
        "the last admin"
    );
    assert!(accounts::delete(&pool, admin.id).await.is_err());

    let (_, token) = accounts::login(&pool, "kid", "kid password").await.unwrap();
    accounts::update(&pool, kid.id, Some("Kid"), Some("new password"), None)
        .await
        .unwrap();
    assert!(
        accounts::session_user(&pool, &token)
            .await
            .unwrap()
            .is_none(),
        "a new password signs out"
    );
    accounts::login(&pool, "kid", "new password").await.unwrap();
    accounts::delete(&pool, kid.id).await.unwrap();
    assert_eq!(accounts::list(&pool).await.unwrap().len(), 1);
}

#[tokio::test]
async fn device_sessions_are_listed_and_last_a_year() {
    let (_dir, pool) = db().await;
    let jo = accounts::setup(&pool, "Jo", "long enough").await.unwrap();
    let (_, web) = accounts::login(&pool, "jo", "long enough").await.unwrap();
    let (user, tv) = accounts::login_device(&pool, "jo", "long enough", " TV ")
        .await
        .unwrap();
    assert_eq!(user.id, jo.id);
    assert!(matches!(
        accounts::login_device(&pool, "jo", "wrong password", "TV").await,
        Err(AccountError::BadLogin)
    ));
    assert!(matches!(
        accounts::login_device(&pool, "jo", "long enough", "  ").await,
        Err(AccountError::Invalid(_))
    ));

    // Only devices are listed; the web session isn't one.
    let devices = accounts::devices(&pool, jo.id).await.unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].name, "TV");

    // Using a session extends it by its own length.
    accounts::session_user(&pool, &web).await.unwrap().unwrap();
    accounts::session_user(&pool, &tv).await.unwrap().unwrap();
    let days: Vec<(Option<String>, i64)> = sqlx::query_as(
        "SELECT device_name, (expires_at - last_seen_at) / 86400 FROM session ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(days[0], (None, accounts::SESSION_DAYS));
    assert_eq!(days[1], (Some("TV".into()), accounts::DEVICE_SESSION_DAYS));

    // Signing out: only your own, only devices.
    let other = accounts::create(&pool, "Sam", "long enough", false)
        .await
        .unwrap();
    assert!(
        !accounts::sign_out_device(&pool, other.id, devices[0].id)
            .await
            .unwrap()
    );
    assert!(
        accounts::sign_out_device(&pool, jo.id, devices[0].id)
            .await
            .unwrap()
    );
    assert!(accounts::session_user(&pool, &tv).await.unwrap().is_none());
    assert!(accounts::session_user(&pool, &web).await.unwrap().is_some());
}

#[tokio::test]
async fn a_new_password_signs_out_devices() {
    let (_dir, pool) = db().await;
    let jo = accounts::setup(&pool, "Jo", "long enough").await.unwrap();
    let (_, tv) = accounts::login_device(&pool, "jo", "long enough", "TV")
        .await
        .unwrap();
    accounts::update(&pool, jo.id, None, Some("another long one"), None)
        .await
        .unwrap();
    assert!(accounts::session_user(&pool, &tv).await.unwrap().is_none());
    assert!(accounts::devices(&pool, jo.id).await.unwrap().is_empty());
}

#[tokio::test]
async fn media_tokens_end_with_their_session() {
    let (_dir, pool) = db().await;
    let jo = accounts::setup(&pool, "Jo", "long enough").await.unwrap();
    let (_, tv) = accounts::login_device(&pool, "jo", "long enough", "TV")
        .await
        .unwrap();
    assert!(
        accounts::create_media_token(&pool, "not a session")
            .await
            .unwrap()
            .is_none()
    );
    let media = accounts::create_media_token(&pool, &tv)
        .await
        .unwrap()
        .unwrap();
    let user = accounts::media_token_user(&pool, &media.token)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user.id, jo.id);

    // Not interchangeable with the session's own token, either way round.
    assert!(
        accounts::session_user(&pool, &media.token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        accounts::media_token_user(&pool, &tv)
            .await
            .unwrap()
            .is_none()
    );

    // An expired one stops working by itself.
    sqlx::query("UPDATE media_token SET expires_at = unixepoch() - 1")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        accounts::media_token_user(&pool, &media.token)
            .await
            .unwrap()
            .is_none()
    );

    let media = accounts::create_media_token(&pool, &tv)
        .await
        .unwrap()
        .unwrap();
    let device = accounts::devices(&pool, jo.id).await.unwrap()[0].id;
    accounts::sign_out_device(&pool, jo.id, device)
        .await
        .unwrap();
    assert!(
        accounts::media_token_user(&pool, &media.token)
            .await
            .unwrap()
            .is_none()
    );
}
