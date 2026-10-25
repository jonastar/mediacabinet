# Benchmarks

## Scanning 50,000 files

`cargo run --release -p cabinet-core --example bench_scan -- <empty folder> 50000`

500 series × 4 seasons × 25 episodes, a subtitle for every fifth episode. Files are tiny, so this measures the walk, file identity and database work, not disk throughput.

Oct 2026, Linux, tmpfs (in memory), release build:

| Case | Time | Result |
| --- | --- | --- |
| First scan (every file hashed) | 3.6 s | 50,000 added, 10,000 sidecars |
| Rescan, nothing changed | 1.4 s | 50,000 unchanged |
| Rescan after renaming one series folder | 1.4 s | 100 moved, links kept |
| Rescan after changing 1% of files | 1.4 s | 500 changed |

Peak memory 80 MB. Database 33 MB.

Notes:

- On spinning disks or network shares the walk dominates; expect several times longer. Scheduled scans and the watcher run in the background, so this affects freshness, not the UI.
- An unchanged rescan still writes `last_seen` for every file and upserts every sidecar. Batching those would cut most of the 1.4 s if it ever matters.
