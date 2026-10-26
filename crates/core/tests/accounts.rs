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
