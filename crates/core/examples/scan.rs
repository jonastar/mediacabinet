//! Scans a folder into a database and prints the report and the files found.
//!
//!   cargo run -p cabinet-core --example scan -- <folder> [database]
//!
//! Without a database path, a temporary one is used. Run twice with the same
//! database to see renames and deletions picked up.

use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = std::fs::canonicalize(args.next().ok_or("usage: scan <folder> [database]")?)?;
    let tmp = tempfile::tempdir()?;
    let db_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| tmp.path().join("cabinet.db"));

    let pool = cabinet_core::db::open(&db_path).await?;
    sqlx::query("INSERT OR IGNORE INTO library (id, name, kind) VALUES (1, 'Scan', 'tv')")
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO library_root (id, library_id, path, layout_template)
         VALUES (1, 1, ?, '{series}/Season {season:00}/{filename}')",
    )
    .bind(root.to_str().ok_or("folder path is not UTF-8")?)
    .execute(&pool)
    .await?;

    let report = cabinet_core::scan::scan_root(&pool, 1).await?;
    let files: Vec<(String, String)> =
        sqlx::query_as("SELECT status, relative_path FROM media_file ORDER BY relative_path")
            .fetch_all(&pool)
            .await?;
    for (status, path) in files {
        println!("{status:8} {path}");
    }
    println!("\n{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
