//! Proposes a layout template from the folders already on a root, for the user
//! to confirm when the root is added.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use utoipa::ToSchema;

use super::group::{group, is_season_folder};
use super::template::{LibraryKind, Template};

static YEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(19|20)\d{2}$").unwrap());
static TRAILING_YEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\(\d{4}\)\s*$").unwrap());
static PADDED_SEASON: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^season 0\d$").unwrap());

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct Proposal {
    pub template: String,
    /// Why this template, in a sentence.
    pub reason: String,
    pub files: usize,
    /// Files the template groups into an item folder.
    pub grouped: usize,
    /// Item folders it finds, e.g. "Night Harbor (2019)".
    pub examples: Vec<String>,
}

/// `paths` are relative video paths under the root.
pub fn propose(paths: &[String], kind: LibraryKind) -> Proposal {
    if paths.is_empty() {
        return finish(
            kind.default_template().to_string(),
            "No videos yet, so this is the default.".into(),
            paths,
            kind,
        );
    }
    let dirs: Vec<Vec<&str>> = paths
        .iter()
        .map(|p| {
            let mut parts: Vec<&str> = p.split('/').collect();
            parts.pop();
            parts
        })
        .collect();

    // TV: the level holding season folders sits below the series folder.
    let mut season_levels: HashMap<usize, usize> = HashMap::new();
    let mut season_names = Vec::new();
    if kind == LibraryKind::Tv {
        for d in &dirs {
            if let Some(i) = d.iter().position(|c| is_season_folder(c)) {
                *season_levels.entry(i).or_default() += 1;
                season_names.push(d[i]);
            }
        }
    }
    let with_seasons: usize = season_levels.values().sum();
    let (item_depth, season_segment) = match most_common(&season_levels) {
        Some(level) if level > 0 && with_seasons * 2 >= paths.len() => {
            let padded = season_names
                .iter()
                .filter(|n| PADDED_SEASON.is_match(n))
                .count();
            let numbered = season_names
                .iter()
                .filter(|n| n.to_lowercase().starts_with("season "))
                .count();
            let segment = if padded * 2 >= numbered {
                "Season {season:00}"
            } else {
                "Season {season}"
            };
            (level - 1, Some(segment))
        }
        _ => {
            // No season folders: the folder holding the files is the item folder.
            let mut depths: HashMap<usize, usize> = HashMap::new();
            for d in dirs.iter().filter(|d| !d.is_empty()) {
                *depths.entry(d.len() - 1).or_default() += 1;
            }
            match most_common(&depths) {
                Some(depth) => (depth, None),
                None => {
                    let reason = "Every video sits directly in the folder, so there are no item folders yet; \
                                  this is the default."
                        .to_string();
                    return finish(kind.default_template().to_string(), reason, paths, kind);
                }
            }
        }
    };

    let mut segments = Vec::new();
    let mut grouping_notes = Vec::new();
    for level in 0..item_depth {
        let values: HashSet<&str> = dirs.iter().filter_map(|d| d.get(level).copied()).collect();
        let segment = if values.len() == 1 {
            values.iter().next().unwrap().to_string()
        } else if values.iter().all(|v| YEAR.is_match(v)) {
            grouping_notes.push("years");
            "{year}".into()
        } else if values.iter().all(|v| v.chars().count() == 1) {
            grouping_notes.push("letters");
            "{letter}".into()
        } else {
            grouping_notes.push("categories");
            "{genre}".into()
        };
        segments.push(segment);
    }

    let item = kind.item_token();
    let item_names: Vec<&str> = dirs
        .iter()
        .filter_map(|d| d.get(item_depth).copied())
        .collect();
    let with_year = item_names
        .iter()
        .filter(|n| TRAILING_YEAR.is_match(n))
        .count();
    segments.push(
        if kind == LibraryKind::Movies && with_year * 2 >= item_names.len().max(1) {
            format!("{{{item}}} ({{year}})")
        } else {
            format!("{{{item}}}")
        },
    );
    if let Some(season) = season_segment {
        segments.push(season.into());
    }
    segments.push("{filename}".into());

    let noun = match kind {
        LibraryKind::Tv => "series",
        LibraryKind::Movies => "movie",
    };
    let levels = |n: usize| {
        if n == 1 {
            "1 level".to_string()
        } else {
            format!("{n} levels")
        }
    };
    let mut reason = match season_segment {
        Some(_) => format!(
            "Season folders sit {} down, so the {noun} folder is the one above them.",
            levels(item_depth + 2)
        ),
        None => format!(
            "Videos sit {} down, in one folder per {noun}.",
            levels(item_depth + 1)
        ),
    };
    if !grouping_notes.is_empty() {
        reason.push_str(&format!(
            " Folders above it group by {}.",
            grouping_notes.join(" and ")
        ));
    }
    finish(segments.join("/"), reason, paths, kind)
}

fn finish(template: String, reason: String, paths: &[String], kind: LibraryKind) -> Proposal {
    let parsed = Template::parse(&template, kind).expect("proposals are valid templates");
    let mut examples = Vec::new();
    let mut seen = HashSet::new();
    let mut grouped = 0;
    for path in paths {
        if let Some(g) = group(&parsed, path) {
            grouped += 1;
            if seen.insert(g.item_folder.clone()) && examples.len() < 5 {
                examples.push(g.item_folder);
            }
        }
    }
    Proposal {
        template,
        reason,
        files: paths.len(),
        grouped,
        examples,
    }
}

fn most_common(counts: &HashMap<usize, usize>) -> Option<usize> {
    counts
        .iter()
        .max_by_key(|(level, count)| (**count, std::cmp::Reverse(**level)))
        .map(|(level, _)| *level)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn default_tv_layout() {
        let p = propose(
            &paths(&[
                "Night Harbor (2019)/Season 01/a.mkv",
                "Night Harbor (2019)/Season 02/b.mkv",
                "Glass Orchard/Season 01/c.mkv",
                "Glass Orchard/Specials/d.mkv",
            ]),
            LibraryKind::Tv,
        );
        assert_eq!(p.template, "{series}/Season {season:00}/{filename}");
        assert_eq!((p.files, p.grouped), (4, 4));
        assert_eq!(p.examples, ["Night Harbor (2019)", "Glass Orchard"]);
    }

    #[test]
    fn grouped_by_year_and_genre() {
        let p = propose(
            &paths(&[
                "2021/Drama/Lantern Street/Season 01/a.mkv",
                "2008/Comedy/Paper Moons/Season 3/b.mkv",
                "2008/Comedy/Paper Moons/Season 3/c.mkv",
            ]),
            LibraryKind::Tv,
        );
        assert_eq!(
            p.template,
            "{year}/{genre}/{series}/Season {season}/{filename}"
        );
        assert!(p.reason.contains("years and categories"), "{}", p.reason);
    }

    #[test]
    fn flat_series_and_constant_levels() {
        let p = propose(
            &paths(&[
                "anime/Starfall Brigade/01.mkv",
                "anime/Starfall Brigade/02.mkv",
                "anime/Kitsune Road/01.mkv",
            ]),
            LibraryKind::Tv,
        );
        assert_eq!(p.template, "anime/{series}/{filename}");
    }

    #[test]
    fn movies_with_years() {
        let p = propose(
            &paths(&[
                "Halfway Coast (2001)/a.mkv",
                "The Quiet Meridian (2014)/b.mkv",
                "loose.mkv",
            ]),
            LibraryKind::Movies,
        );
        assert_eq!(p.template, "{movie} ({year})/{filename}");
        assert_eq!((p.files, p.grouped), (3, 2));
    }

    #[test]
    fn empty_or_flat_roots_get_the_default() {
        assert_eq!(
            propose(&[], LibraryKind::Tv).template,
            "{series}/Season {season:00}/{filename}"
        );
        assert_eq!(
            propose(&paths(&["a.mkv"]), LibraryKind::Movies).template,
            "{movie} ({year})/{filename}"
        );
    }
}
