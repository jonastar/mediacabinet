//! Scan time and memory on a large fake library.
//!
//!   cargo run --release -p cabinet-core --example bench_scan -- <empty folder> [files]
//!
//! Writes `files` small videos (default 50,000) as 500 series × 4 seasons, plus a
//! subtitle for every fifth episode, then times a first scan, an unchanged rescan,
//! a rescan after renaming one series folder (25 seasons' worth of moves), and a
//! rescan after touching every 100th file. Files are small, so this measures the
//! walk, identity and database work, not disk throughput.

use std::fs;
use std::path::Path;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let base = std::path::PathBuf::from(
        args.next()
            .ok_or("usage: bench_scan <empty folder> [files]")?,
    );
    let files: usize = args
        .next()
        .map(|n| n.parse())
        .transpose()?
        .unwrap_or(50_000);
    let root = base.join("tv");
    let per_series = 100;
    let series = files.div_ceil(per_series);

    let started = Instant::now();
    let mut written = 0;
    'outer: for s in 0..series {
        for season in 1..=4 {
            let dir = root.join(format!("Series {s:04}/Season {season:02}"));
            fs::create_dir_all(&dir)?;
            for e in 1..=per_series / 4 {
                if written == files {
                    break 'outer;
                }
                let name = format!("Series {s:04} - S{season:02}E{e:02}");
                fs::write(
                    dir.join(format!("{name}.mkv")),
                    format!("{name} ").repeat(64),
                )?;
                if e % 5 == 0 {
                    fs::write(
                        dir.join(format!("{name}.en.srt")),
                        "1\n00:00:00,000 --> 00:00:01,000\nHi\n",
                    )?;
                }
                written += 1;
            }
        }
    }
    println!(
        "wrote {written} videos in {:.1}s",
        started.elapsed().as_secs_f64()
    );

    let pool = cabinet_core::db::open(&base.join("cabinet.db")).await?;
    sqlx::query("INSERT INTO library (id, name, kind) VALUES (1, 'TV', 'tv')")
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO library_root (id, library_id, path, layout_template) VALUES (1, 1, ?, ?)",
    )
    .bind(root.to_string_lossy().into_owned())
    .bind(cabinet_core::layout::DEFAULT_TV)
    .execute(&pool)
    .await?;

    let scan = |label: &'static str| {
        let pool = pool.clone();
        async move {
            let t = Instant::now();
            let report = cabinet_core::scan::scan_root(&pool, 1).await?;
            println!(
                "{label:<28} {:>6.2}s  added {:>6}  moved {:>5}  changed {:>4}  unchanged {:>6}  sidecars {:>6}",
                t.elapsed().as_secs_f64(),
                report.added,
                report.moved,
                report.changed,
                report.unchanged,
                report.sidecars
            );
            Ok::<_, Box<dyn std::error::Error>>(())
        }
    };

    scan("first scan").await?;
    scan("rescan, nothing changed").await?;
    fs::rename(root.join("Series 0000"), root.join("Series 0000 (2019)"))?;
    scan("rescan, one series renamed").await?;
    touch_every(&root, 100)?;
    scan("rescan, 1% of files changed").await?;

    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    let peak = status
        .lines()
        .find(|l| l.starts_with("VmHWM"))
        .unwrap_or("VmHWM: unknown");
    println!("peak memory {}", peak.trim_start_matches("VmHWM:").trim());
    let db_size = fs::metadata(base.join("cabinet.db"))?.len();
    println!("database {:.1} MB", db_size as f64 / 1e6);
    Ok(())
}

/// Appends to every `n`th video, changing its size and content.
fn touch_every(root: &Path, n: usize) -> std::io::Result<()> {
    let mut i = 0;
    for entry in walk(root)? {
        if entry.extension().is_some_and(|e| e == "mkv") {
            if i % n == 0 {
                use std::io::Write;
                fs::OpenOptions::new()
                    .append(true)
                    .open(&entry)?
                    .write_all(b" changed")?;
            }
            i += 1;
        }
    }
    Ok(())
}

fn walk(dir: &Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            out.extend(walk(&path)?);
        } else {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}
