//! Database connection and migrations.

use std::path::{Path, PathBuf};
use std::time::Duration;

use sqlx::migrate::Migrator;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

/// Opens (or creates) the database at `path` and applies pending migrations.
///
/// An existing database is copied to a timestamped backup before any migration runs.
pub async fn open(path: &Path) -> Result<SqlitePool, DbError> {
    let existed = path.exists();
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;

    if existed && has_pending_migrations(&pool).await {
        let backup = backup_path(path);
        tracing::info!(backup = %backup.display(), "backing up database before migrating");
        sqlx::query("VACUUM INTO ?")
            .bind(backup.to_string_lossy().into_owned())
            .execute(&pool)
            .await?;
    }

    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

async fn has_pending_migrations(pool: &SqlitePool) -> bool {
    // A missing migrations table means nothing has been applied yet.
    let applied: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success")
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    MIGRATOR.iter().any(|m| !applied.contains(&m.version))
}

/// Starts a transaction that will write. `BEGIN IMMEDIATE` takes the write lock
/// up front, so concurrent writers wait (up to the busy timeout) instead of
/// failing with "database is locked" when a read would upgrade to a write.
pub async fn begin_write(
    pool: &SqlitePool,
) -> Result<sqlx::Transaction<'static, sqlx::Sqlite>, sqlx::Error> {
    pool.begin_with("BEGIN IMMEDIATE").await
}

fn backup_path(path: &Path) -> PathBuf {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{stamp}.bak"));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn opens_and_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cabinet.db");

        let pool = open(&path).await.unwrap();
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(mode, "wal");
        pool.close().await;

        open(&path).await.unwrap().close().await;
        assert_eq!(backups(dir.path()), 0, "nothing pending, so no backup");
    }

    #[tokio::test]
    async fn backs_up_existing_database_before_migrating() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cabinet.db");

        // An existing database that has not had any migrations applied.
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        sqlx::query("CREATE TABLE keep_me (x INTEGER)")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        open(&path).await.unwrap().close().await;
        assert_eq!(backups(dir.path()), 1);
    }

    fn backups(dir: &Path) -> usize {
        std::fs::read_dir(dir)
            .unwrap()
            .filter(|e| e.as_ref().unwrap().path().extension() == Some("bak".as_ref()))
            .count()
    }
}
