//! Finds each file's item folder (series or movie) from the root's template, with
//! hints for identification. Grouping levels are hints only, never metadata.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use utoipa::ToSchema;

use super::template::Template;

/// Inline provider IDs: `{tmdb-1399}`, `[tmdbid-1399]`, `{imdb-tt0944947}`.
static INLINE_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s*[\[{](tmdb|tvdb|imdb|anidb)(?:id)?-([a-z0-9]+)[\]}]").unwrap()
});
static TRAILING_YEAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*\((\d{4})\)\s*$").unwrap());
static SEASON_FOLDER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:season|series|staffel|saison|temporada|s)[\s._-]*(\d{1,3})$").unwrap()
});

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct Hints {
    /// The item's name from its folder, without year or IDs.
    pub title: String,
    pub year: Option<i64>,
    pub genre: Option<String>,
    pub season: Option<i64>,
    /// (provider, id) pairs found in the path.
    pub ids: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// Relative path of the item folder.
    pub item_folder: String,
    pub hints: Hints,
}

/// Groups one file (`relative_path` under the root). None when the file sits above
/// the template's item level, e.g. a loose file in a TV root.
pub fn group(template: &Template, relative_path: &str) -> Option<Group> {
    let components: Vec<&str> = relative_path.split('/').collect();
    let item = template.item_index;
    // The item folder must be a folder: the file is somewhere below it.
    if components.len() < item + 2 {
        return None;
    }

    let mut hints = Hints::default();
    for (i, component) in components[..item].iter().enumerate() {
        if let Some(caps) = template.captures(i, component) {
            hints.year = hints
                .year
                .or_else(|| caps.get("year").and_then(|y| y.parse().ok()));
            hints.genre = hints.genre.or_else(|| caps.get("genre").cloned());
        }
    }

    let folder = components[item];
    let mut title = folder.to_string();
    if let Some(caps) = template.captures(item, folder) {
        if let Some(name) = caps.get("series").or_else(|| caps.get("movie")) {
            title = name.clone();
        }
        if let Some(year) = caps.get("year").and_then(|y| y.parse().ok()) {
            hints.year = Some(year);
        }
    }
    for caps in INLINE_ID.captures_iter(&title) {
        hints
            .ids
            .push((caps[1].to_lowercase(), caps[2].to_string()));
    }
    title = INLINE_ID.replace_all(&title, "").to_string();
    if let Some(caps) = TRAILING_YEAR.captures(&title) {
        hints.year = hints.year.or_else(|| caps[1].parse().ok());
        title = TRAILING_YEAR.replace(&title, "").to_string();
    }
    hints.title = title.trim().to_string();

    // A season folder below the item folder.
    for (offset, component) in components[item + 1..components.len() - 1]
        .iter()
        .enumerate()
    {
        let from_template = template
            .captures(item + 1 + offset, component)
            .and_then(|caps| caps.get("season").and_then(|s| s.parse().ok()));
        let generic = SEASON_FOLDER
            .captures(component)
            .and_then(|c| c[1].parse().ok());
        let specials = component.eq_ignore_ascii_case("specials").then_some(0);
        if let Some(season) = from_template.or(generic).or(specials) {
            hints.season = Some(season);
            break;
        }
    }

    Some(Group {
        item_folder: components[..=item].join("/"),
        hints,
    })
}

/// A folder name with its inline provider IDs set to `ids` (provider, value),
/// keeping each tag's style. None when every tag already agrees, or names a
/// provider `ids` doesn't have.
pub fn corrected_ids(name: &str, ids: &[(String, String)]) -> Option<String> {
    let mut changed = false;
    let fixed = INLINE_ID.replace_all(name, |caps: &regex::Captures| {
        let whole = &caps[0];
        let provider = caps[1].to_lowercase();
        match ids.iter().find(|(p, _)| *p == provider) {
            Some((_, value)) if !value.eq_ignore_ascii_case(&caps[2]) => {
                changed = true;
                let at = caps.get(2).unwrap().start() - caps.get(0).unwrap().start();
                format!("{}{value}{}", &whole[..at], &whole[at + caps[2].len()..])
            }
            _ => whole.to_string(),
        }
    });
    changed.then(|| fixed.into_owned())
}

pub(crate) fn is_season_folder(name: &str) -> bool {
    SEASON_FOLDER.is_match(name) || name.eq_ignore_ascii_case("specials")
}

#[cfg(test)]
mod tests {
    use super::super::template::LibraryKind;
    use super::*;

    fn tv(s: &str) -> Template {
        Template::parse(s, LibraryKind::Tv).unwrap()
    }

    #[test]
    fn default_layout() {
        let g = group(
            &tv("{series}/Season {season:00}/{filename}"),
            "Night Harbor (2019)/Season 01/e01.mkv",
        )
        .unwrap();
        assert_eq!(g.item_folder, "Night Harbor (2019)");
        assert_eq!(g.hints.title, "Night Harbor");
        assert_eq!((g.hints.year, g.hints.season), (Some(2019), Some(1)));
    }

    #[test]
    fn grouping_levels_are_hints() {
        let t = tv("{year}/{genre}/{series}/Season {season:00}/{filename}");
        let g = group(
            &t,
            "2021/Drama/Lantern Street/Season 01/Lantern.Street.S01E01.mkv",
        )
        .unwrap();
        assert_eq!(g.item_folder, "2021/Drama/Lantern Street");
        assert_eq!(g.hints.title, "Lantern Street");
        assert_eq!(
            (g.hints.year, g.hints.genre.as_deref()),
            (Some(2021), Some("Drama"))
        );
    }

    #[test]
    fn inline_ids_and_specials() {
        let t = tv("{series}/{filename}");
        let g = group(&t, "Glass Orchard {tmdb-99001}/Season 1/x.mp4").unwrap();
        assert_eq!(g.hints.title, "Glass Orchard");
        assert_eq!(g.hints.ids, [("tmdb".to_string(), "99001".to_string())]);
        assert_eq!(
            g.hints.season,
            Some(1),
            "season folders are found even when the template has none"
        );
        let g = group(&t, "Show [tvdbid-123] (2001)/Specials/x.mkv").unwrap();
        assert_eq!(
            (g.hints.title.as_str(), g.hints.year, g.hints.season),
            ("Show", Some(2001), Some(0))
        );
    }

    #[test]
    fn corrects_inline_ids_in_their_own_style() {
        let ids = [
            ("tmdb".to_string(), "456".to_string()),
            ("imdb".to_string(), "tt0944947".to_string()),
        ];
        assert_eq!(
            corrected_ids("Show (2001) [tmdb-123123]", &ids).as_deref(),
            Some("Show (2001) [tmdb-456]")
        );
        assert_eq!(
            corrected_ids("Show {tmdbid-123} {imdb-tt1}", &ids).as_deref(),
            Some("Show {tmdbid-456} {imdb-tt0944947}")
        );
        assert_eq!(corrected_ids("Show [tmdb-456]", &ids), None);
        assert_eq!(
            corrected_ids("Show {tvdb-9}", &ids),
            None,
            "no tvdb ID to compare"
        );
        assert_eq!(corrected_ids("Show", &ids), None);
    }

    #[test]
    fn files_above_the_item_level_have_no_group() {
        let t = tv("{year}/{series}/{filename}");
        assert_eq!(group(&t, "2021/loose.mkv"), None);
        assert_eq!(group(&t, "loose.mkv"), None);
    }

    #[test]
    fn item_folder_pattern_with_year() {
        let t = Template::parse("{movie} ({year})/{filename}", LibraryKind::Movies).unwrap();
        let g = group(
            &t,
            "The Quiet Meridian (2014)/The Quiet Meridian (2014).mkv",
        )
        .unwrap();
        assert_eq!(
            (g.hints.title.as_str(), g.hints.year),
            ("The Quiet Meridian", Some(2014))
        );
        // A folder without a year still groups; the whole name is the title.
        let g = group(&t, "Unsorted Film/x.mkv").unwrap();
        assert_eq!(
            (g.hints.title.as_str(), g.hints.year),
            ("Unsorted Film", None)
        );
    }
}
