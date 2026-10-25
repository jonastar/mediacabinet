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
        let dir = std::env::temp_dir().join(format!("cabinet-db-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cabinet.db");

        let pool = open(&path).await.unwrap();
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(mode, "wal");
        pool.close().await;

        open(&path).await.unwrap().close().await;
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
