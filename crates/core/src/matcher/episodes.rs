//! Resolves a parsed filename to episodes of one series, through its orderings.
//! Pure: the series is loaded into a `SeriesIndex` first.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use super::score::{similarity, title_within};
use crate::metadata::fake::normalize;
use crate::parse::Parsed;

#[derive(Debug, Clone)]
pub struct OrderingIndex {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub is_default: bool,
    /// (season, number) → items. Several items can share a number (merged episodes).
    pub by_number: HashMap<(Option<i64>, i64), Vec<i64>>,
}

#[derive(Debug, Clone)]
pub struct EpisodeInfo {
    pub title: String,
    pub airs_at: Option<i64>,
    pub runtime_seconds: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct SeriesIndex {
    pub series_id: i64,
    pub title: String,
    /// The series' matching ordering, if set.
    pub match_ordering: Option<i64>,
    pub orderings: Vec<OrderingIndex>,
    pub episodes: HashMap<i64, EpisodeInfo>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Resolution {
    pub items: Vec<i64>,
    pub confidence: f64,
    pub reason: String,
    /// Which part of the episode the file is, when it's split over several files.
    pub part: Option<i64>,
}

/// One file of a folder, for `resolve_parts`.
pub struct FileToResolve<'a> {
    pub parsed: &'a Parsed,
    pub folder_season: Option<i64>,
    pub duration: Option<f64>,
}

impl SeriesIndex {
    /// Orderings to try for season numbers: the matching ordering, the default,
    /// then the rest.
    fn seasonal_order(&self) -> Vec<&OrderingIndex> {
        let mut list: Vec<&OrderingIndex> = self
            .orderings
            .iter()
            .filter(|o| o.kind != "absolute")
            .collect();
        list.sort_by_key(|o| (Some(o.id) != self.match_ordering, !o.is_default, o.id));
        list
    }

    fn absolute_order(&self) -> Vec<&OrderingIndex> {
        let mut list: Vec<&OrderingIndex> = self
            .orderings
            .iter()
            .filter(|o| o.kind == "absolute")
            .collect();
        list.sort_by_key(|o| (Some(o.id) != self.match_ordering, o.id));
        list
    }

    /// An item's season and number in the first seasonal ordering that has it.
    fn position(&self, item: i64) -> Option<(Option<i64>, i64)> {
        self.seasonal_order().into_iter().find_map(|o| {
            o.by_number
                .iter()
                .find(|(_, items)| items.contains(&item))
                .map(|(key, _)| *key)
        })
    }

    fn lookup(o: &OrderingIndex, season: Option<i64>, numbers: &[i64]) -> Option<Vec<i64>> {
        let mut items = Vec::new();
        for n in numbers {
            items.extend(o.by_number.get(&(season, *n))?.iter().copied());
        }
        items.dedup();
        Some(items)
    }
}

/// Which episodes a file is, and how sure. `folder_season` is a season folder
/// above the file, if any; `duration` is the probed length in seconds.
pub fn resolve(
    index: &SeriesIndex,
    parsed: &Parsed,
    folder_season: Option<i64>,
    duration: Option<f64>,
) -> Option<Resolution> {
    let (items, base, mut reason) = numbers(index, parsed, folder_season)?;
    let mut confidence = base;

    // Sanity checks.
    let expected: i64 = items
        .iter()
        .filter_map(|i| index.episodes.get(i).and_then(|e| e.runtime_seconds))
        .sum();
    if let Some(d) = duration
        && expected > 0
    {
        let ratio = d / expected as f64;
        if !(0.5..=1.6).contains(&ratio) {
            confidence -= 0.25;
            reason.push_str(&format!(
                "; runtime {} min, expected {} min",
                (d / 60.0).round(),
                expected / 60
            ));
        }
    }
    if let Some(title) = &parsed.episode_title
        && items.len() == 1
        && let Some(e) = index.episodes.get(&items[0])
    {
        let s = similarity(title, &e.title);
        if s >= 0.8 || title_within(title, &e.title) {
            confidence += 0.04;
            reason.push_str("; episode title matches");
        } else if s < 0.3 && !e.title.starts_with("Episode ") {
            confidence -= 0.15;
            reason.push_str(&format!(
                "; episode title \"{title}\" differs from \"{}\"",
                e.title
            ));
        }
    }
    Some(Resolution {
        items,
        confidence: confidence.clamp(0.0, 1.0),
        reason,
        part: None,
    })
}

/// "Part 2" at the end of an episode title.
static TITLE_PART: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)[\s,:-]*\b(?:part|pt)\.?[ ]?\d{1,2}$").unwrap());

/// Episodes split over several files where the series has them as one: "The Last
/// Day (1)" and "(2)" as S08E09 and S08E10 against a single S08E09, or "Part 1" and
/// "Part 2" under one number. Files are grouped by season and episode title (or
/// their numbers, without a title); each group's episode is the one their numbers
/// find with that title, or else the only episode with it. Returns the parts'
/// resolutions by their position in `files`; other files are left to `resolve`.
pub fn resolve_parts(index: &SeriesIndex, files: &[FileToResolve]) -> HashMap<usize, Resolution> {
    let mut groups: HashMap<(Option<i64>, String), Vec<usize>> = HashMap::new();
    for (i, f) in files.iter().enumerate() {
        if f.parsed.part.is_none() {
            continue;
        }
        let key = match base_title(f.parsed) {
            Some(t) => normalize(&t),
            None => format!("{:?}{:?}", f.parsed.episodes, f.parsed.absolute),
        };
        groups.entry((f.parsed.season, key)).or_default().push(i);
    }

    let mut out = HashMap::new();
    for members in groups.into_values() {
        let mut parts: Vec<i64> = members
            .iter()
            .filter_map(|i| files[*i].parsed.part)
            .collect();
        parts.sort();
        // Two files for one part are copies, not a split.
        if parts.windows(2).any(|w| w[0] == w[1]) {
            continue;
        }
        let title = base_title(files[members[0]].parsed);
        let titled = |item: &i64| {
            title.as_deref().is_none_or(|t| {
                index
                    .episodes
                    .get(item)
                    .is_some_and(|e| similarity(t, &e.title) >= 0.8 || title_within(t, &e.title))
            })
        };
        let own: Vec<Option<Resolution>> = members
            .iter()
            .map(|i| resolve(index, files[*i].parsed, files[*i].folder_season, None))
            .collect();
        // A lone part its numbers place is just that episode.
        if members.len() == 1 && own[0].is_some() {
            continue;
        }
        let mut found: Vec<i64> = own
            .iter()
            .flatten()
            .filter(|r| r.items.len() == 1)
            .map(|r| r.items[0])
            .filter(titled)
            .collect();
        found.sort();
        found.dedup();
        let (episode, base) = match found.as_slice() {
            [one] => (*one, 0.95),
            // Nothing by number: the only episode of that title.
            [] => {
                let Some(t) = &title else {
                    continue;
                };
                let hits: Vec<i64> = index
                    .episodes
                    .iter()
                    .filter(|(_, e)| similarity(t, &e.title) >= 0.8)
                    .map(|(id, _)| *id)
                    .collect();
                match hits.as_slice() {
                    [one] => (*one, 0.85),
                    _ => continue,
                }
            }
            // The series splits it too: each part is its own episode.
            _ => continue,
        };

        let Some(info) = index.episodes.get(&episode) else {
            continue;
        };
        let label = match index.position(episode) {
            Some((Some(s), n)) => format!("S{s:02}E{n:02}"),
            Some((None, n)) => format!("{n:02}"),
            None => "the episode".to_string(),
        };
        let mut confidence: f64 = base;
        if title.is_some() {
            confidence += 0.04;
        }
        let mut note = String::new();
        let complete = parts.len() > 1 && parts.iter().copied().eq(1..=parts.len() as i64);
        if !complete {
            confidence = confidence.min(0.8);
            note.push_str(&format!("; only part {} found", list(&parts)));
        }
        let durations: Option<Vec<f64>> = members.iter().map(|i| files[*i].duration).collect();
        if let (Some(d), Some(expected)) = (durations, info.runtime_seconds)
            && expected > 0
        {
            let total: f64 = d.iter().sum();
            if !(0.5..=1.6).contains(&(total / expected as f64)) {
                confidence -= 0.25;
                note.push_str(&format!(
                    "; together {} min, expected {} min",
                    (total / 60.0).round(),
                    expected / 60
                ));
            }
        }
        for i in members {
            let part = files[i].parsed.part.expect("grouped by part");
            out.insert(
                i,
                Resolution {
                    items: vec![episode],
                    confidence: confidence.clamp(0.0, 1.0),
                    reason: format!(
                        "part {part} of {label} \"{}\", split over {} files{note}",
                        info.title,
                        parts.len()
                    ),
                    part: Some(part),
                },
            );
        }
    }
    out
}

/// The episode title without a "Part 2" at its end, if anything is left.
fn base_title(parsed: &Parsed) -> Option<String> {
    let t = parsed.episode_title.as_deref()?;
    let t = TITLE_PART.replace(t, "");
    (!t.trim().is_empty()).then(|| t.trim().to_string())
}

fn list(parts: &[i64]) -> String {
    let parts: Vec<String> = parts.iter().map(i64::to_string).collect();
    parts.join(" and ")
}

fn numbers(
    index: &SeriesIndex,
    parsed: &Parsed,
    folder_season: Option<i64>,
) -> Option<(Vec<i64>, f64, String)> {
    let label = |s: i64, eps: &[i64]| {
        let list: Vec<String> = eps.iter().map(|e| format!("E{e:02}")).collect();
        format!("S{s:02}{}", list.join(""))
    };

    if let (Some(season), false) = (parsed.season, parsed.episodes.is_empty()) {
        for (i, o) in index.seasonal_order().iter().enumerate() {
            if let Some(items) = SeriesIndex::lookup(o, Some(season), &parsed.episodes) {
                let base = if i == 0 {
                    0.95
                } else {
                    0.85
                };
                return Some((
                    items,
                    base,
                    format!("{} in {} order", label(season, &parsed.episodes), o.name),
                ));
            }
        }
        // An episode past the end of a season that exists often means absolute numbering.
        let season_end = index
            .seasonal_order()
            .iter()
            .flat_map(|o| o.by_number.keys())
            .filter(|(s, _)| *s == Some(season))
            .map(|(_, n)| *n)
            .max();
        if parsed.episodes.len() == 1 && season_end.is_some_and(|end| parsed.episodes[0] > end) {
            for o in index.absolute_order() {
                // Past the end of a season, an absolute number is past that season too:
                // S08E10 is never absolute 10.
                if let Some(items) = SeriesIndex::lookup(o, None, &parsed.episodes)
                    && items.iter().all(|i| {
                        index
                            .position(*i)
                            .is_some_and(|(s, _)| s.is_some_and(|s| s > season))
                    })
                {
                    return Some((
                        items,
                        0.8,
                        format!(
                            "{} isn't in any season; read as {} in {}",
                            label(season, &parsed.episodes),
                            parsed.episodes[0],
                            o.name
                        ),
                    ));
                }
            }
        }
        return None;
    }

    if let Some(date) = parsed.date {
        let day = date.and_hms_opt(12, 0, 0)?.and_utc().timestamp();
        let hits: Vec<i64> = index
            .episodes
            .iter()
            .filter(|(_, e)| e.airs_at.is_some_and(|t| (t - day).abs() <= 36 * 3600))
            .map(|(id, _)| *id)
            .collect();
        if hits.len() == 1 {
            return Some((hits, 0.9, format!("aired on {date}")));
        }
        return None;
    }

    if !parsed.absolute.is_empty() {
        let n = &parsed.absolute;
        // In a season folder, a bare number is most likely the episode in that season.
        if let Some(season) = folder_season {
            let default = index.seasonal_order().into_iter().next();
            if let Some(o) = default
                && let Some(items) = SeriesIndex::lookup(o, Some(season), n)
            {
                let fits_absolute = index
                    .absolute_order()
                    .iter()
                    .any(|a| SeriesIndex::lookup(a, None, n).is_some_and(|abs| abs != items));
                let base = if fits_absolute {
                    0.75
                } else {
                    0.88
                };
                return Some((
                    items,
                    base,
                    format!("{} from the season folder", label(season, n)),
                ));
            }
        }
        for o in index.absolute_order() {
            if let Some(items) = SeriesIndex::lookup(o, None, n) {
                let base = if Some(o.id) == index.match_ordering {
                    0.95
                } else {
                    0.9
                };
                return Some((items, base, format!("{} in {} order", n[0], o.name)));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    /// Two seasons of 12 in aired order (items 101–112, 201–212), an absolute
    /// order 1–24, and a DVD order where S01 has 13 episodes (items shift).
    fn index() -> SeriesIndex {
        let mut aired = HashMap::new();
        let mut absolute = HashMap::new();
        let mut episodes = HashMap::new();
        for s in 1..=2 {
            for e in 1..=12 {
                let id = s * 100 + e;
                aired.insert((Some(s), e), vec![id]);
                absolute.insert((None, (s - 1) * 12 + e), vec![id]);
                episodes.insert(
                    id,
                    EpisodeInfo {
                        title: format!("Title {id}"),
                        airs_at: Some(1_600_000_000 + id * 86400),
                        runtime_seconds: Some(24 * 60),
                    },
                );
            }
        }
        let mut dvd = HashMap::new();
        dvd.insert((Some(1), 13), vec![201]);
        SeriesIndex {
            series_id: 1,
            title: "Starfall Brigade".into(),
            match_ordering: None,
            orderings: vec![
                OrderingIndex {
                    id: 1,
                    name: "Aired".into(),
                    kind: "aired".into(),
                    is_default: true,
                    by_number: aired,
                },
                OrderingIndex {
                    id: 2,
                    name: "Absolute".into(),
                    kind: "absolute".into(),
                    is_default: false,
                    by_number: absolute,
                },
                OrderingIndex {
                    id: 3,
                    name: "DVD".into(),
                    kind: "dvd".into(),
                    is_default: false,
                    by_number: dvd,
                },
            ],
            episodes,
        }
    }

    fn items(name: &str, folder_season: Option<i64>) -> Option<(Vec<i64>, f64)> {
        resolve(&index(), &parse(name), folder_season, None).map(|r| (r.items, r.confidence))
    }

    #[test]
    fn season_numbers_in_the_default_ordering() {
        assert_eq!(items("Show - S02E03.mkv", None), Some((vec![203], 0.95)));
        assert_eq!(items("Show.S01E01E02.mkv", None).unwrap().0, [101, 102]);
    }

    #[test]
    fn other_orderings_when_the_default_has_no_such_number() {
        let (items, confidence) = items("Show - S01E13.mkv", None).unwrap();
        assert_eq!(items, [201]);
        assert!(confidence < 0.9, "found in DVD order, not the default");
    }

    #[test]
    fn absolute_numbers() {
        assert_eq!(
            items("[Kumo] Starfall Brigade - 14 [1080p].mkv", None),
            Some((vec![202], 0.9))
        );
        // In a season folder, a bare number is the episode of that season.
        assert_eq!(
            items("Starfall Brigade - 03.mkv", Some(2)).unwrap().0,
            [203]
        );
    }

    #[test]
    fn runtime_and_title_checks_adjust_confidence() {
        let short = resolve(&index(), &parse("Show - S01E01.mkv"), None, Some(120.0)).unwrap();
        assert!(
            short.confidence < 0.85 && short.reason.contains("runtime 2 min"),
            "{short:?}"
        );
        let titled = resolve(
            &index(),
            &parse("Show - S01E01 - Title 101.mkv"),
            None,
            Some(1440.0),
        )
        .unwrap();
        assert_eq!(titled.confidence, 0.99);
        let wrong = resolve(
            &index(),
            &parse("Show - S01E01 - Something Else.mkv"),
            None,
            None,
        )
        .unwrap();
        assert!(wrong.confidence < 0.85, "{wrong:?}");
        // Sources name episodes differently: a short title within a long one matches.
        let longer = resolve(
            &index(),
            &parse("Show - S01E01 - The Extended Title 101 Cut.mkv"),
            None,
            None,
        )
        .unwrap();
        assert!(
            longer.reason.contains("episode title matches"),
            "{longer:?}"
        );
    }

    fn parts(names: &[(&str, f64)], index: &SeriesIndex) -> Vec<Option<(i64, i64, f64)>> {
        let parsed: Vec<Parsed> = names.iter().map(|(n, _)| parse(n)).collect();
        let files: Vec<FileToResolve> = parsed
            .iter()
            .zip(names)
            .map(|(p, (_, minutes))| FileToResolve {
                parsed: p,
                folder_season: None,
                duration: Some(minutes * 60.0),
            })
            .collect();
        let mut found = resolve_parts(index, &files);
        (0..names.len())
            .map(|i| {
                found
                    .remove(&i)
                    .map(|r| (r.items[0], r.part.unwrap(), r.confidence))
            })
            .collect()
    }

    #[test]
    fn past_a_season_is_not_an_earlier_absolute_number() {
        // Absolute 13 is S02E01, before the end of season 2.
        assert_eq!(items("Show - S02E13.mkv", None), None);
    }

    #[test]
    fn an_episode_split_over_two_files() {
        let mut index = index();
        index.episodes.get_mut(&212).unwrap().title = "The Last Day".into();
        // The second half numbered one past the end of the season.
        let found = parts(
            &[
                ("Show - S02E12 - The Last Day (1) WEBDL-1080p.mkv", 12.0),
                ("Show - S02E13 - The Last Day (2) WEBDL-1080p.mkv", 12.0),
            ],
            &index,
        );
        assert_eq!(found, [Some((212, 1, 0.99)), Some((212, 2, 0.99))]);
        // Or under one number.
        let found = parts(
            &[
                ("Show - S02E05 - Title 205 Part 1.mkv", 12.0),
                ("Show - S02E05 - Title 205 Part 2.mkv", 12.0),
            ],
            &index,
        );
        assert_eq!(found, [Some((205, 1, 0.99)), Some((205, 2, 0.99))]);
        // Halves that together aren't the episode's length go to review.
        let found = parts(
            &[
                ("Show - S02E05 - Title 205 Part 1.mkv", 24.0),
                ("Show - S02E05 - Title 205 Part 2.mkv", 24.0),
            ],
            &index,
        );
        assert!(found[0].unwrap().2 < 0.85, "{found:?}");
        // One half alone goes to review too.
        let found = parts(&[("Show - S02E13 - The Last Day (2).mkv", 12.0)], &index);
        assert_eq!(found[0].map(|f| (f.0, f.1)), Some((212, 2)));
        assert!(found[0].unwrap().2 < 0.85);
    }

    #[test]
    fn halves_the_series_has_as_episodes() {
        let mut index = index();
        index.episodes.get_mut(&103).unwrap().title = "The Last Day (1)".into();
        index.episodes.get_mut(&104).unwrap().title = "The Last Day (2)".into();
        let found = parts(
            &[
                ("Show - S01E03 - The Last Day (1).mkv", 24.0),
                ("Show - S01E04 - The Last Day (2).mkv", 24.0),
            ],
            &index,
        );
        assert_eq!(found, [None, None]);
        // A lone part its number places is that episode, not a split.
        assert_eq!(
            parts(&[("Show - S01E03 - The Last Day (1).mkv", 24.0)], &index),
            [None]
        );
    }

    #[test]
    fn nothing_to_go_on() {
        assert_eq!(items("Show - Special Feature.mkv", None), None);
        assert_eq!(items("Show - S05E01.mkv", None), None);
    }
}
