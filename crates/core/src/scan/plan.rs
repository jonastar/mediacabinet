//! Decides what a walk means for the files already known on a root.
//!
//! Pure apart from hashing, so it can be tested without a database. Each found
//! file is matched to an existing row, in order:
//!
//! 1. Same device and inode, with the same size and mtime: the same file, possibly
//!    renamed. Inode goes before path so that two swapped files keep their links.
//! 2. Same path: the same file. If size or mtime changed, its content changed.
//! 3. Same content hash as a row that wasn't found: moved, e.g. copied across
//!    devices and the original deleted.
//!
//! Anything left over is new. Known files that weren't found become missing.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;

use super::walk::{Found, Walk};

#[derive(Debug, Clone)]
pub struct Existing {
    pub id: i64,
    pub relative_path: String,
    pub size: i64,
    pub mtime: i64,
    pub device_id: Option<i64>,
    pub inode: Option<i64>,
    pub content_hash: Option<String>,
    /// `present`, `missing` or `ignored`. Removed rows are not loaded.
    pub status: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MatchKind {
    Same,
    Moved {
        from: String,
    },
    /// Same path, different content. Carries the new hash, if it could be read.
    Changed {
        hash: Option<String>,
    },
}

#[derive(Debug)]
pub struct Matched {
    pub id: i64,
    pub found: Found,
    pub kind: MatchKind,
    pub was_missing: bool,
}

#[derive(Debug, Default)]
pub struct Plan {
    pub matched: Vec<Matched>,
    pub added: Vec<(Found, String)>,
    /// Rows that were present and weren't found: (id, relative path).
    pub missing: Vec<(i64, String)>,
    /// New files that couldn't be read for hashing; skipped until the next scan.
    pub unreadable_new: usize,
}

fn claim(
    plan: &mut Plan,
    unclaimed: &mut HashMap<i64, Existing>,
    id: i64,
    found: Found,
    kind: MatchKind,
) {
    let row = unclaimed.remove(&id).expect("claimed row is unclaimed");
    plan.matched.push(Matched {
        id,
        found,
        kind,
        was_missing: row.status == "missing",
    });
}

pub fn plan(
    existing: Vec<Existing>,
    walk: Walk,
    root: &Path,
    hash: impl Fn(&Path) -> io::Result<String>,
) -> Plan {
    let mut plan = Plan::default();

    // Rows under unreadable folders are left exactly as they are.
    let mut unclaimed: HashMap<i64, Existing> = existing
        .into_iter()
        .filter(|row| !walk.is_unreadable(&row.relative_path))
        .map(|row| (row.id, row))
        .collect();
    let by_inode: HashMap<(i64, i64), i64> = unclaimed
        .values()
        .filter_map(|row| Some(((row.device_id?, row.inode?), row.id)))
        .collect();
    let by_path: HashMap<String, i64> = unclaimed
        .values()
        .map(|row| (row.relative_path.clone(), row.id))
        .collect();

    // 1. Inode.
    let mut rest = Vec::new();
    for found in walk.files {
        let key = found.device_id.zip(found.inode);
        let row = key
            .and_then(|k| by_inode.get(&k))
            .and_then(|id| unclaimed.get(id))
            .filter(|row| row.size == found.size && row.mtime == found.mtime);
        match row {
            Some(row) => {
                let id = row.id;
                let kind = if row.relative_path == found.relative_path {
                    MatchKind::Same
                } else {
                    MatchKind::Moved {
                        from: row.relative_path.clone(),
                    }
                };
                claim(&mut plan, &mut unclaimed, id, found, kind);
            }
            None => {
                rest.push(found);
            }
        }
    }

    // 2. Path.
    let mut unmatched = Vec::new();
    for found in rest {
        let row = by_path
            .get(&found.relative_path)
            .and_then(|id| unclaimed.get(id));
        match row {
            Some(row) => {
                let id = row.id;
                let kind = if row.size == found.size && row.mtime == found.mtime {
                    MatchKind::Same
                } else {
                    MatchKind::Changed {
                        hash: hash(&root.join(&found.relative_path)).ok(),
                    }
                };
                claim(&mut plan, &mut unclaimed, id, found, kind);
            }
            None => {
                unmatched.push(found);
            }
        }
    }

    // 3. Content hash, against rows that weren't found. Also hashes every new file.
    let mut by_hash: HashMap<String, Vec<i64>> = HashMap::new();
    for row in unclaimed.values() {
        if let Some(h) = &row.content_hash {
            by_hash.entry(h.clone()).or_default().push(row.id);
        }
    }
    let mut taken = HashSet::new();
    for found in unmatched {
        let Ok(h) = hash(&root.join(&found.relative_path)) else {
            plan.unreadable_new += 1;
            continue;
        };
        let row_id = by_hash
            .get(&h)
            .and_then(|ids| ids.iter().find(|id| !taken.contains(*id)))
            .copied();
        match row_id {
            Some(id) => {
                taken.insert(id);
                let from = unclaimed[&id].relative_path.clone();
                claim(
                    &mut plan,
                    &mut unclaimed,
                    id,
                    found,
                    MatchKind::Moved { from },
                );
            }
            None => {
                plan.added.push((found, h));
            }
        }
    }

    // 4. Known files not found. Ignored rows keep their status.
    plan.missing = unclaimed
        .into_values()
        .filter(|row| row.status == "present")
        .map(|row| (row.id, row.relative_path))
        .collect();
    plan.missing.sort();
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(path: &str, inode: i64, size: i64) -> Found {
        Found {
            relative_path: path.into(),
            size,
            mtime: 100,
            device_id: Some(1),
            inode: Some(inode),
        }
    }

    fn row(id: i64, path: &str, inode: i64, size: i64, hash: &str) -> Existing {
        Existing {
            id,
            relative_path: path.into(),
            size,
            mtime: 100,
            device_id: Some(1),
            inode: Some(inode),
            content_hash: Some(hash.into()),
            status: "present".into(),
        }
    }

    /// Hashes by file name, so tests control which files "have the same content".
    fn hash_by_name(path: &Path) -> io::Result<String> {
        let name = path.file_stem().unwrap().to_str().unwrap();
        Ok(name.trim_end_matches("-copy").to_string())
    }

    fn walk(files: Vec<Found>) -> Walk {
        Walk {
            files,
            ..Walk::default()
        }
    }

    fn kind_of(plan: &Plan, id: i64) -> &MatchKind {
        &plan.matched.iter().find(|m| m.id == id).unwrap().kind
    }

    #[test]
    fn swapped_files_follow_their_inode() {
        let existing = vec![row(1, "e01.mkv", 10, 5, "a"), row(2, "e02.mkv", 20, 6, "b")];
        let found = vec![found("e01.mkv", 20, 6), found("e02.mkv", 10, 5)];
        let plan = plan(existing, walk(found), Path::new("/"), hash_by_name);
        assert_eq!(
            kind_of(&plan, 1),
            &MatchKind::Moved {
                from: "e01.mkv".into()
            }
        );
        assert_eq!(
            plan.matched
                .iter()
                .find(|m| m.id == 1)
                .unwrap()
                .found
                .relative_path,
            "e02.mkv"
        );
        assert!(plan.added.is_empty() && plan.missing.is_empty());
    }

    #[test]
    fn same_path_with_new_content_is_changed() {
        let existing = vec![row(1, "e01.mkv", 10, 5, "old")];
        let plan = plan(
            existing,
            walk(vec![found("e01.mkv", 11, 9)]),
            Path::new("/"),
            hash_by_name,
        );
        assert_eq!(
            kind_of(&plan, 1),
            &MatchKind::Changed {
                hash: Some("e01".into())
            }
        );
    }

    #[test]
    fn copied_across_devices_matches_by_hash() {
        let existing = vec![row(1, "old/e01.mkv", 10, 5, "e01")];
        let plan = plan(
            existing,
            walk(vec![found("new/e01.mkv", 99, 5)]),
            Path::new("/"),
            hash_by_name,
        );
        assert_eq!(
            kind_of(&plan, 1),
            &MatchKind::Moved {
                from: "old/e01.mkv".into()
            }
        );
    }

    #[test]
    fn a_copy_of_a_present_file_is_new() {
        let existing = vec![row(1, "e01.mkv", 10, 5, "e01")];
        let found = vec![found("e01.mkv", 10, 5), found("e01-copy.mkv", 11, 5)];
        let plan = plan(existing, walk(found), Path::new("/"), hash_by_name);
        assert_eq!(kind_of(&plan, 1), &MatchKind::Same);
        assert_eq!(plan.added.len(), 1);
    }

    #[test]
    fn unreadable_folders_mark_nothing_missing() {
        let existing = vec![
            row(1, "locked/e01.mkv", 10, 5, "a"),
            row(2, "open/e02.mkv", 20, 5, "b"),
        ];
        let walk = Walk {
            unreadable: vec!["locked".into()],
            ..Walk::default()
        };
        let plan = plan(existing, walk, Path::new("/"), hash_by_name);
        assert_eq!(plan.missing, vec![(2, "open/e02.mkv".into())]);
    }

    #[test]
    fn ignored_files_are_never_marked_missing() {
        let mut ignored = row(1, "sample.mkv", 10, 5, "a");
        ignored.status = "ignored".into();
        let plan = plan(vec![ignored], walk(vec![]), Path::new("/"), hash_by_name);
        assert!(plan.missing.is_empty());
    }
}
