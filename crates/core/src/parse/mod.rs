//! Reads what a filename says: title, year, season and episodes, absolute
//! numbers, episode title, edition, part, extras and technical tags.
//!
//! The parser only reports what the name says. Deciding which item a file is
//! belongs to the matcher, which also knows the folder and the orderings.
//!
//! Approach: pull out bracketed IDs, editions, hashes and the release group;
//! find the first episode marker (in priority order); the title is what comes
//! before it, the episode title what follows up to the first technical tag.
//! Without a marker, the last plausible year ends a movie title.

use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;
use serde::Serialize;
use utoipa::ToSchema;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Extra {
    Sample,
    Trailer,
    Featurette,
    BehindTheScenes,
    DeletedScene,
    Interview,
    Short,
    Other,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct Parsed {
    pub title: Option<String>,
    pub year: Option<i64>,
    pub season: Option<i64>,
    /// Episode numbers within `season`; several for multi-episode files.
    pub episodes: Vec<i64>,
    /// Absolute numbers, when the name has no season (typical for anime).
    pub absolute: Vec<i64>,
    /// Date-numbered episodes (daily shows).
    #[schema(value_type = Option<String>)]
    pub date: Option<NaiveDate>,
    pub episode_title: Option<String>,
    pub edition: Option<String>,
    /// For movies split over several files: cd1, part2…
    pub part: Option<i64>,
    pub version: Option<i64>,
    pub resolution: Option<String>,
    pub release_group: Option<String>,
    pub extra: Option<Extra>,
    /// Provider IDs written in the name: (provider, id).
    pub ids: Vec<(String, String)>,
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("parser patterns are valid")
}

static EXTENSION: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\.[a-z0-9]{2,4}$"));
static INLINE_ID: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)[\[{](tmdb|tvdb|imdb|anidb)(?:id)?[-=]([a-z0-9]+)[\]}]"));
static EDITION_TAG: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\{edition-([^}]+)\}"));
static LEADING_GROUP: LazyLock<Regex> = LazyLock::new(|| re(r"^\[([^\]]+)\]\s*"));
static HASH: LazyLock<Regex> = LazyLock::new(|| re(r"\[[0-9A-Fa-f]{8}\]"));
static TRAILING_GROUP: LazyLock<Regex> = LazyLock::new(|| re(r"-([A-Za-z0-9]+)$"));
static RESOLUTION: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\b(2160|1440|1080|720|576|480)[pi]\b|\b(4k|uhd)\b"));
static VERSION: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b\d{1,4}v(\d)\b|\bv(\d)\b"));
static EXTRA_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)[-_.](trailer|sample|featurette|behindthescenes|deleted|deletedscene|interview|scene|short|other|extra)$",
    )
});
static SAMPLE_PREFIX: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^sample[-_.]"));
static SAMPLE_WORD: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\bsample\b"));

/// Technical words that end a title or an episode title.
static TAG: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?ix)
        \b(?:2160|1440|1080|720|576|480)[pi]\b | \b(?:4k|uhd|hdr|hdr10|dv|sdr)\b
        | \b(?:web[\x20-]?dl|webrip|web|bluray|blu[\x20-]?ray|bdrip|brrip|bd|hdtv|dvdrip|dvd|remux|hdrip)\b
        | \b(?:x26[45]|h\x20?26[45]|hevc|avc|av1|xvid|divx|10bit|8bit)\b
        | \b(?:aac|ac3|eac3|ddp?|dts|truehd|atmos|flac|opus)(?:\x20?\d\x20\d)?\b
        | \b(?:proper|repack|internal|limited|multi|dubbed|subbed)\b
        | \[ | \(
    ")
});

static EDITIONS: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b(director'?s[ ]cut|extended(?:[ ](?:edition|cut))?|unrated|uncut|theatrical(?:[ ]cut)?|remastered|imax|special[ ]edition|final[ ]cut|ultimate[ ]edition|criterion)\b",
    )
});

// Episode markers, in priority order.
static SXXEXX: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\bs(\d{1,3})[ ]?e(\d{1,4})((?:[ -]?e\d{1,4})*)(?:-(\d{1,4})\b)?"));
static NXNN: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\b(\d{1,2})x(\d{2,3})((?:[x-]\d{2,3})*)\b"));
static WORDS: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\bseason[ ]?(\d{1,3})\W+episode[ ]?(\d{1,4})\b"));
static SEASON_DASH: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\bs(\d{1,2})[ ]?-[ ]?(\d{1,4})\b"));
static DAILY: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b((?:19|20)\d{2})[ .-](\d{2})[ .-](\d{2})\b"));
static DASH_NUMBER: LazyLock<Regex> = LazyLock::new(|| re(r"\s-\s(\d{1,4})(?:v\d)?(?:\s|$)"));
static EPISODE_WORD: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\b(?:episode|ep|e)[ ]?(\d{1,4})\b"));
static BARE_NUMBER: LazyLock<Regex> = LazyLock::new(|| re(r"\s(\d{2,4})(?:v\d)?\s*(?:\[|\(|$)"));

static YEAR: LazyLock<Regex> = LazyLock::new(|| re(r"\(?\b((?:19|20)\d{2})\b\)?"));
static PART: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\b(?:cd|disc|disk|dvd|part|pt)[ ]?(\d{1,2})\b"));
/// "The Last Day (1)": a numbered half of a two-part episode.
static EPISODE_PART: LazyLock<Regex> = LazyLock::new(|| re(r"^\((\d)\)"));

/// Parses a file name (no folders).
pub fn parse(filename: &str) -> Parsed {
    let mut p = Parsed::default();
    let mut s = EXTENSION.replace(filename, "").to_string();

    for caps in INLINE_ID.captures_iter(&s) {
        p.ids.push((caps[1].to_lowercase(), caps[2].to_string()));
    }
    s = INLINE_ID.replace_all(&s, " ").to_string();
    if let Some(caps) = EDITION_TAG.captures(&s) {
        p.edition = Some(caps[1].trim().to_string());
    }
    s = EDITION_TAG.replace_all(&s, " ").to_string();

    if let Some(caps) = EXTRA_SUFFIX.captures(&s) {
        p.extra = Some(extra_kind(&caps[1]));
        s = EXTRA_SUFFIX.replace(&s, "").to_string();
    } else if SAMPLE_PREFIX.is_match(&s) {
        p.extra = Some(Extra::Sample);
        s = SAMPLE_PREFIX.replace(&s, "").to_string();
    }

    if let Some(caps) = LEADING_GROUP.captures(&s) {
        p.release_group = Some(caps[1].to_string());
        s = LEADING_GROUP.replace(&s, "").to_string();
    }
    s = HASH.replace_all(&s, " ").to_string();

    if let Some(caps) = RESOLUTION.captures(&s) {
        p.resolution = Some(match caps.get(1) {
            Some(n) => format!("{}p", n.as_str()),
            None => "2160p".into(),
        });
    }
    if let Some(caps) = VERSION.captures(&s) {
        p.version = caps
            .get(1)
            .or(caps.get(2))
            .and_then(|v| v.as_str().parse().ok());
    }
    // Scene names end in -GROUP after their tags.
    let scene = !s.contains(' ') && s.contains('.');
    if p.release_group.is_none()
        && scene
        && let Some(caps) = TRAILING_GROUP.captures(&s)
    {
        let before = &s[..caps.get(0).unwrap().start()];
        if TAG.is_match(&spaced(before)) {
            p.release_group = Some(caps[1].to_string());
            s = before.to_string();
        }
    }

    let text = spaced(&s);
    let marker = find_marker(&text, &mut p);
    let (head, tail) = match marker {
        Some((start, end)) => (&text[..start], &text[end..]),
        None => {
            let year = movie_year(&text);
            match year {
                Some((year, start, end)) => {
                    p.year = Some(year);
                    (&text[..start], &text[end..])
                }
                None => {
                    let cut = TAG.find(&text).map_or(text.len(), |m| m.start());
                    (&text[..cut], &text[cut..])
                }
            }
        }
    };

    // Title, with a trailing year ("Doctor Who (2005)") moved to `year`.
    let mut title = clean(head);
    if marker.is_some()
        && let Some(caps) = YEAR.captures_iter(&title).last()
    {
        let m = caps.get(0).unwrap();
        if m.end() == title.len() && m.start() > 0 {
            p.year = caps[1].parse().ok();
            title = clean(&title[..m.start()]);
        }
    }
    if let Some(m) = EDITIONS.find(&title) {
        p.edition.get_or_insert_with(|| edition_name(m.as_str()));
        title = clean(&title[..m.start()]);
    }
    p.title = (!title.is_empty()).then_some(title);

    // After the marker or year: edition, part, sample, episode title.
    if let Some(m) = EDITIONS.find(tail) {
        p.edition.get_or_insert_with(|| edition_name(m.as_str()));
    }
    if let Some(caps) = PART.captures(tail) {
        p.part = caps[1].parse().ok();
    }
    if p.extra.is_none() && SAMPLE_WORD.is_match(tail) {
        p.extra = Some(Extra::Sample);
    }
    if marker.is_some() {
        let cut = TAG.find(tail).map_or(tail.len(), |m| m.start());
        let name = clean(&tail[..cut]);
        if !name.is_empty() && !SAMPLE_WORD.is_match(&name) {
            p.episode_title = Some(name);
            if let Some(caps) = EPISODE_PART.captures(&tail[cut..]) {
                p.part.get_or_insert(caps[1].parse().unwrap_or(1));
            }
        }
    }
    p
}

/// Separators to spaces: dots and underscores, except inside numbers like 5.1.
fn spaced(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    for (i, c) in chars.iter().enumerate() {
        let between_digits = i > 0
            && i + 1 < chars.len()
            && chars[i - 1].is_ascii_digit()
            && chars[i + 1].is_ascii_digit();
        if (*c == '.' && !between_digits) || *c == '_' {
            out.push(' ');
        } else {
            out.push(*c);
        }
    }
    out
}

fn clean(s: &str) -> String {
    let s = s.trim_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '(' | '[' | ',' | '+'));
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn expand(first: i64, rest: &str, range_end: Option<i64>, prefix: char) -> Vec<i64> {
    let mut eps = vec![first];
    for n in rest
        .split(|c: char| c.eq_ignore_ascii_case(&prefix) || c == '-' || c == ' ')
        .filter_map(|x| x.parse().ok())
    {
        eps.push(n);
    }
    if let Some(end) = range_end {
        eps.extend(first + 1..=end);
    }
    // "E01-E03" is a range, not two episodes.
    if eps.len() == 2 && rest.contains('-') && eps[1] > eps[0] + 1 {
        eps = (eps[0]..=eps[1]).collect();
    }
    eps.sort();
    eps.dedup();
    eps
}

/// Finds the first episode marker and records it. Returns its span in `text`.
fn find_marker(text: &str, p: &mut Parsed) -> Option<(usize, usize)> {
    if let Some(c) = SXXEXX.captures(text) {
        let first: i64 = c[2].parse().ok()?;
        let range_end = c.get(4).and_then(|m| m.as_str().parse().ok());
        p.season = c[1].parse().ok();
        p.episodes = expand(first, c.get(3).map_or("", |m| m.as_str()), range_end, 'e');
        let m = c.get(0).unwrap();
        return Some((m.start(), m.end()));
    }
    if let Some(c) = NXNN.captures(text) {
        p.season = c[1].parse().ok();
        p.episodes = expand(
            c[2].parse().ok()?,
            c.get(3).map_or("", |m| m.as_str()),
            None,
            'x',
        );
        let m = c.get(0).unwrap();
        return Some((m.start(), m.end()));
    }
    if let Some(c) = WORDS.captures(text) {
        p.season = c[1].parse().ok();
        p.episodes = vec![c[2].parse().ok()?];
        let m = c.get(0).unwrap();
        return Some((m.start(), m.end()));
    }
    if let Some(c) = SEASON_DASH.captures(text) {
        p.season = c[1].parse().ok();
        p.episodes = vec![c[2].parse().ok()?];
        let m = c.get(0).unwrap();
        return Some((m.start(), m.end()));
    }
    if let Some(c) = DAILY.captures(text) {
        let date =
            NaiveDate::from_ymd_opt(c[1].parse().ok()?, c[2].parse().ok()?, c[3].parse().ok()?);
        if date.is_some() {
            p.date = date;
            let m = c.get(0).unwrap();
            return Some((m.start(), m.end()));
        }
    }
    for pattern in [&*DASH_NUMBER, &*EPISODE_WORD, &*BARE_NUMBER] {
        if let Some(c) = pattern.captures(text) {
            let m = c.get(0).unwrap();
            let n: i64 = c[1].parse().ok()?;
            // Needs a title before it, and a four-digit number must not be a year.
            if m.start() == 0 || (c[1].len() == 4 && (1900..2100).contains(&n)) {
                continue;
            }
            p.absolute = vec![n];
            return Some((m.start(), m.end()));
        }
    }
    None
}

/// The last plausible year that isn't the start of the title ("1917 (2019)").
fn movie_year(text: &str) -> Option<(i64, usize, usize)> {
    let tag = TAG.find(text).map_or(text.len(), |m| m.start());
    let candidates: Vec<(i64, usize, usize)> = YEAR
        .captures_iter(text)
        .filter_map(|c| {
            let m = c.get(0).unwrap();
            let y: i64 = c[1].parse().ok()?;
            (m.start() > 0 && m.start() <= tag.max(m.start())).then_some((y, m.start(), m.end()))
        })
        .collect();
    // Prefer a year in parentheses, then the last one.
    candidates
        .iter()
        .rev()
        .find(|(_, s, _)| text[*s..].starts_with('('))
        .or(candidates.last())
        .copied()
}

fn extra_kind(tag: &str) -> Extra {
    match tag.to_lowercase().as_str() {
        "trailer" => Extra::Trailer,
        "sample" => Extra::Sample,
        "featurette" => Extra::Featurette,
        "behindthescenes" => Extra::BehindTheScenes,
        "deleted" | "deletedscene" | "scene" => Extra::DeletedScene,
        "interview" => Extra::Interview,
        "short" => Extra::Short,
        _ => Extra::Other,
    }
}

fn edition_name(raw: &str) -> String {
    let lower = raw.to_lowercase();
    match lower.as_str() {
        s if s.starts_with("director") => "Director's Cut".into(),
        "imax" => "IMAX".into(),
        _ => raw
            .split_whitespace()
            .map(|w| {
                let mut c = w.chars();
                c.next()
                    .map(|f| f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}
