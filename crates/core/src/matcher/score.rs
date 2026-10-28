//! Small scoring helpers: title similarity and `.nfo` IDs.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use crate::metadata::fake::normalize;

/// 0–1 similarity between two titles, ignoring case, punctuation and "the".
/// Dice coefficient over character bigrams of the normalised strings.
pub fn similarity(a: &str, b: &str) -> f64 {
    let strip = |s: &str| {
        let n = normalize(s);
        n.strip_prefix("the ").map(str::to_string).unwrap_or(n)
    };
    let (a, b) = (strip(a), strip(b));
    if a == b {
        return 1.0;
    }
    let grams = |s: &str| -> HashSet<(char, char)> {
        let chars: Vec<char> = s.chars().filter(|c| *c != ' ').collect();
        chars.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let (ga, gb) = (grams(&a), grams(&b));
    if ga.is_empty() || gb.is_empty() {
        return 0.0;
    }
    let shared = ga.intersection(&gb).count() as f64;
    2.0 * shared / (ga.len() + gb.len()) as f64
}

/// Whether the shorter title appears in the longer one as whole words, like
/// "Diet" in "The South Bronx Paradise Diet": one source's short title for an
/// episode another source names in full.
pub fn title_within(a: &str, b: &str) -> bool {
    let (a, b) = (normalize(a), normalize(b));
    let (short, long) = if a.len() <= b.len() {
        (a, b)
    } else {
        (b, a)
    };
    let short: Vec<&str> = short.split(' ').collect();
    let long: Vec<&str> = long.split(' ').collect();
    // Too short to mean anything ("a", "ep").
    if short.concat().chars().count() < 3 {
        return false;
    }
    long.windows(short.len()).any(|w| w == short.as_slice())
}

static UNIQUE_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<uniqueid[^>]*\btype\s*=\s*["']?(tmdb|tvdb|imdb|anidb)["']?[^>]*>\s*([^<\s]+)\s*</uniqueid>"#).unwrap()
});
static LEGACY_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<(tmdbid|tvdbid|imdbid|imdb_id)>\s*([^<\s]+)\s*</").unwrap()
});
static IMDB_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"imdb\.com/title/(tt\d+)").unwrap());

/// Provider IDs in an `.nfo` file (Kodi XML, or a bare IMDb link).
pub fn nfo_ids(text: &str) -> Vec<(String, String)> {
    let mut ids: Vec<(String, String)> = UNIQUE_ID
        .captures_iter(text)
        .map(|c| (c[1].to_lowercase(), c[2].to_string()))
        .collect();
    for c in LEGACY_ID.captures_iter(text) {
        let provider = match c[1].to_lowercase().as_str() {
            "tmdbid" => "tmdb",
            "tvdbid" => "tvdb",
            _ => "imdb",
        };
        ids.push((provider.to_string(), c[2].to_string()));
    }
    for c in IMDB_URL.captures_iter(text) {
        ids.push(("imdb".to_string(), c[1].to_string()));
    }
    let mut seen = HashSet::new();
    ids.retain(|id| seen.insert(id.clone()));
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similar_titles() {
        assert_eq!(similarity("The Quiet Meridian", "quiet.meridian"), 1.0);
        assert_eq!(similarity("Signal & Noise", "Signal and Noise"), 1.0);
        assert!(similarity("Night Harbor", "Night Harbour") > 0.85);
        assert!(similarity("Night Harbor", "Glass Orchard") < 0.3);
    }

    #[test]
    fn titles_within_titles() {
        assert!(title_within("Diet", "The South Bronx Paradise Diet"));
        assert!(title_within("The South Bronx Paradise Diet", "diet"));
        assert!(title_within(
            "Paradise Diet",
            "The South Bronx Paradise Diet"
        ));
        assert!(!title_within("Die", "The South Bronx Paradise Diet"));
        assert!(!title_within("Diet Plan", "The South Bronx Paradise Diet"));
        assert!(!title_within("a", "A Night Out"));
    }

    #[test]
    fn reads_nfo_ids() {
        let nfo = r#"<tvshow><uniqueid type="tmdb" default="true">99010</uniqueid>
                     <uniqueid type="tvdb">399010</uniqueid></tvshow>"#;
        assert_eq!(
            nfo_ids(nfo),
            [
                ("tmdb".into(), "99010".into()),
                ("tvdb".into(), "399010".into())
            ]
        );
        assert_eq!(
            nfo_ids("<movie><imdbid>tt0123</imdbid></movie>"),
            [("imdb".into(), "tt0123".into())]
        );
        assert_eq!(
            nfo_ids("https://www.imdb.com/title/tt7654321/"),
            [("imdb".into(), "tt7654321".into())]
        );
    }
}
