//! Resolves a parsed filename to episodes of one series, through its orderings.
//! Pure: the series is loaded into a `SeriesIndex` first.

use std::collections::HashMap;

use super::score::similarity;
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
        if s >= 0.8 {
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
    })
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
                if let Some(items) = SeriesIndex::lookup(o, None, &parsed.episodes) {
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
    }

    #[test]
    fn nothing_to_go_on() {
        assert_eq!(items("Show - Special Feature.mkv", None), None);
        assert_eq!(items("Show - S05E01.mkv", None), None);
    }
}
