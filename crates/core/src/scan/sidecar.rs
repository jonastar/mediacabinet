//! Assigns sidecar files (subtitles, `.nfo`, artwork) to the videos they belong to.
//!
//! Folder-level files such as `tvshow.nfo`, `poster.jpg` or `fanart.jpg` describe a
//! series or movie, not one file, and are left for series identification.

use std::collections::HashMap;
use std::path::Path;

const SUBTITLE_EXTENSIONS: &[&str] = &["ass", "idx", "srt", "ssa", "sub", "sup", "vtt"];
const ARTWORK_EXTENSIONS: &[&str] = &["jpeg", "jpg", "png", "tbn", "webp"];

/// (ISO 639-1, ISO 639-2/B, English name). Codes are stored as 639-2, like ffprobe.
const LANGUAGES: &[(&str, &str, &str)] = &[
    ("ar", "ara", "arabic"),
    ("cs", "cze", "czech"),
    ("da", "dan", "danish"),
    ("de", "ger", "german"),
    ("el", "gre", "greek"),
    ("en", "eng", "english"),
    ("es", "spa", "spanish"),
    ("fi", "fin", "finnish"),
    ("fr", "fre", "french"),
    ("he", "heb", "hebrew"),
    ("hi", "hin", "hindi"),
    ("hu", "hun", "hungarian"),
    ("it", "ita", "italian"),
    ("ja", "jpn", "japanese"),
    ("ko", "kor", "korean"),
    ("nl", "dut", "dutch"),
    ("no", "nor", "norwegian"),
    ("pl", "pol", "polish"),
    ("pt", "por", "portuguese"),
    ("ro", "rum", "romanian"),
    ("ru", "rus", "russian"),
    ("sv", "swe", "swedish"),
    ("th", "tha", "thai"),
    ("tr", "tur", "turkish"),
    ("uk", "ukr", "ukrainian"),
    ("zh", "chi", "chinese"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assigned {
    pub relative_path: String,
    /// Relative path of the video it belongs to.
    pub video: String,
    pub kind: &'static str,
    pub language: Option<String>,
    pub is_forced: bool,
    pub is_hearing_impaired: bool,
}

pub fn kind_of(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if SUBTITLE_EXTENSIONS.contains(&ext.as_str()) {
        Some("subtitle")
    } else if ext == "nfo" {
        Some("nfo")
    } else if ARTWORK_EXTENSIONS.contains(&ext.as_str()) {
        Some("artwork")
    } else {
        None
    }
}

/// Assigns each sidecar to at most one video. `videos` and `sidecars` are relative paths.
pub fn assign(videos: &[String], sidecars: &[String]) -> Vec<Assigned> {
    let mut by_folder: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
    for video in videos {
        let (folder, name) = split(video);
        by_folder
            .entry(folder)
            .or_default()
            .push((stem(name), video));
    }

    let mut assigned = Vec::new();
    for sidecar in sidecars {
        let Some(kind) = kind_of(Path::new(sidecar)) else {
            continue;
        };
        let (folder, name) = split(sidecar);
        let (video_folder, hint) = subs_folder(folder).unwrap_or((folder, None));
        let Some(candidates) = by_folder.get(video_folder) else {
            continue;
        };

        // The longest video name the sidecar's name starts with, so that
        // "Film - Director's Cut.en.srt" doesn't go to "Film.mkv".
        // In `Subs/<episode>/`, the folder name is the video's name exactly.
        let by_name = candidates
            .iter()
            .filter(|(stem, _)| match hint {
                Some(hint) => hint == *stem,
                None => belongs_to(name, stem, kind),
            })
            .max_by_key(|(stem, _)| stem.len());
        let only = (candidates.len() == 1).then(|| &candidates[0]);
        let video = match kind {
            "subtitle" => by_name.or(only),
            "nfo" if name.eq_ignore_ascii_case("movie.nfo") => only,
            _ => by_name,
        };
        let Some((video_stem, video)) = video else {
            continue;
        };

        let tags = if hint.is_some() || !belongs_to(name, video_stem, kind) {
            stem(name)
        } else {
            name[video_stem.len()..].trim_start_matches(['.', '-', '_'])
        };
        let (language, is_forced, is_hearing_impaired) = if kind == "subtitle" {
            subtitle_tags(stem(tags))
        } else {
            (None, false, false)
        };
        assigned.push(Assigned {
            relative_path: sidecar.clone(),
            video: video.to_string(),
            kind,
            language,
            is_forced,
            is_hearing_impaired,
        });
    }
    assigned
}

fn split(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

fn stem(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

/// `name` is `stem` plus a separator and more: `.en.srt`, or `-poster.jpg` for artwork.
fn belongs_to(name: &str, stem: &str, kind: &str) -> bool {
    let Some(rest) = name.strip_prefix(stem) else {
        return false;
    };
    rest.starts_with('.') || (kind == "artwork" && rest.starts_with('-'))
}

/// `Video/Subs/x.srt` → (`Video`, None); `Video/Subs/<episode>/x.srt` → (`Video`, Some(episode)).
fn subs_folder(folder: &str) -> Option<(&str, Option<&str>)> {
    let is_subs =
        |name: &str| name.eq_ignore_ascii_case("subs") || name.eq_ignore_ascii_case("subtitles");
    let (parent, last) = split(folder);
    if is_subs(last) {
        return Some((parent, None));
    }
    let (grandparent, middle) = split(parent);
    if is_subs(middle) {
        return Some((grandparent, Some(last)));
    }
    None
}

/// Reads `en.forced`, `eng.sdh`, `2_English` and similar.
fn subtitle_tags(tags: &str) -> (Option<String>, bool, bool) {
    let mut language = None;
    let mut forced = false;
    let mut hearing_impaired = false;
    for token in tags.split(['.', '_', ' ', '-', '[', ']', '(', ')']) {
        let token = token.to_ascii_lowercase();
        match token.as_str() {
            "forced" | "foreign" => {
                forced = true;
            }
            "sdh" | "hi" | "cc" => {
                hearing_impaired = true;
            }
            _ => {
                if language.is_none() {
                    language = normalize_language(&token);
                }
            }
        }
    }
    (language, forced, hearing_impaired)
}

fn normalize_language(token: &str) -> Option<String> {
    LANGUAGES
        .iter()
        .find(|(two, three, name)| token == *two || token == *three || token == *name)
        .map(|(_, three, _)| three.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn one(videos: &[&str], sidecar: &str) -> Option<Assigned> {
        assign(&paths(videos), &paths(&[sidecar]))
            .into_iter()
            .next()
    }

    #[test]
    fn subtitles_next_to_the_video() {
        let s = one(&["S1/Show - S01E01.mkv"], "S1/Show - S01E01.fr.forced.srt").unwrap();
        assert_eq!(s.video, "S1/Show - S01E01.mkv");
        assert_eq!((s.language.as_deref(), s.is_forced), (Some("fre"), true));

        let s = one(&["S1/a.mkv"], "S1/a.eng.sdh.srt").unwrap();
        assert_eq!(
            (s.language.as_deref(), s.is_hearing_impaired),
            (Some("eng"), true)
        );
    }

    #[test]
    fn longest_matching_name_wins() {
        let videos = ["F/Film.mkv", "F/Film - Director's Cut.mkv"];
        assert_eq!(
            one(&videos, "F/Film - Director's Cut.en.srt")
                .unwrap()
                .video,
            "F/Film - Director's Cut.mkv"
        );
        assert_eq!(one(&videos, "F/Film.en.srt").unwrap().video, "F/Film.mkv");
    }

    #[test]
    fn unnamed_subtitle_goes_to_the_only_video() {
        assert_eq!(
            one(&["F/Film.mkv"], "F/English.srt")
                .unwrap()
                .language
                .as_deref(),
            Some("eng")
        );
        assert_eq!(one(&["F/a.mkv", "F/b.mkv"], "F/English.srt"), None);
    }

    #[test]
    fn subs_folders() {
        let s = one(&["F/Film.mkv"], "F/Subs/2_English.srt").unwrap();
        assert_eq!(
            (s.video.as_str(), s.language.as_deref()),
            ("F/Film.mkv", Some("eng"))
        );

        let videos = ["S/Show.S01E01.mkv", "S/Show.S01E02.mkv"];
        let s = one(&videos, "S/Subs/Show.S01E02/3_Spanish.srt").unwrap();
        assert_eq!(
            (s.video.as_str(), s.language.as_deref()),
            ("S/Show.S01E02.mkv", Some("spa"))
        );
    }

    #[test]
    fn artwork_and_nfo_by_name_only() {
        assert_eq!(
            one(&["S/e01.mkv"], "S/e01-thumb.jpg").unwrap().kind,
            "artwork"
        );
        assert_eq!(one(&["S/e01.mkv"], "S/e01.nfo").unwrap().kind, "nfo");
        assert_eq!(
            one(&["M/Film.mkv"], "M/movie.nfo").unwrap().video,
            "M/Film.mkv"
        );
        // Series and folder art belong to identification, not to a file.
        assert_eq!(one(&["S/e01.mkv"], "S/tvshow.nfo"), None);
        assert_eq!(one(&["M/Film.mkv"], "M/poster.jpg"), None);
    }

    #[test]
    fn name_prefix_needs_a_separator() {
        assert_eq!(
            one(&["S/e1.mkv", "S/e10.mkv"], "S/e10.en.srt")
                .unwrap()
                .video,
            "S/e10.mkv"
        );
        assert_eq!(one(&["S/e1.mkv", "S/e2.mkv"], "S/e10.en.srt"), None);
    }
}
