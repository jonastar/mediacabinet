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
    let prober = cabinet_core::scan::Prober::default();
    let probed = cabinet_core::scan::probe_root(&pool, 1, &prober).await?;
    let files: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT f.status, f.relative_path,
                coalesce((SELECT group_concat(
                     s.kind || ':' || coalesce(s.codec, '?')
                     || coalesce(' ' || s.width || 'x' || s.height, '')
                     || coalesce(' ' || s.hdr_format, '')
                     || coalesce(' ' || s.channels || 'ch', '')
                     || coalesce(' ' || s.language, '')
                     || CASE WHEN s.is_forced THEN ' forced' ELSE '' END, ', ')
                     FROM media_stream s WHERE s.file_id = f.id), f.probe_error, '')
                || coalesce((SELECT ' | sidecars: ' || group_concat(
                     replace(c.relative_path, rtrim(c.relative_path, replace(c.relative_path, '/', '')), ''), ', ')
                     FROM sidecar c WHERE c.file_id = f.id), '')
         FROM media_file f ORDER BY f.relative_path",
    )
    .fetch_all(&pool)
    .await?;
    for (status, path, details) in files {
        println!("{status:8} {path}\n         {details}");
    }
    println!("\n{}", serde_json::to_string_pretty(&report)?);
    println!("{}", serde_json::to_string_pretty(&probed)?);
    Ok(())
}
