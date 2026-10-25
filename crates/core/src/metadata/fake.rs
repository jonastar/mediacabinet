//! An offline catalog of series and movies, loaded from JSON. Used by tests, and
//! by `CABINET_METADATA_CATALOG` to try the app without a provider account.

use serde::Deserialize;

use super::model::{MovieData, SearchResult, SeriesData};

#[derive(Debug, Default, Deserialize)]
pub struct Catalog {
    #[serde(default)]
    pub series: Vec<SeriesData>,
    #[serde(default)]
    pub movies: Vec<MovieData>,
}

/// Lowercase letters and digits only, so "Signal & Noise" matches "signal and noise"
/// loosely and "Night.Harbor" matches "Night Harbor".
pub(crate) fn normalize(s: &str) -> String {
    s.to_lowercase()
        .replace('&', " and ")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

impl Catalog {
    pub fn from_json(json: &str) -> Result<Catalog, serde_json::Error> {
        serde_json::from_str(json)
    }

    fn search<'a>(
        items: impl Iterator<
            Item = (
                &'a str,
                &'a str,
                Option<&'a str>,
                Option<i64>,
                Option<&'a str>,
            ),
        >,
        query: &str,
        year: Option<i64>,
    ) -> Vec<SearchResult> {
        let q = normalize(query);
        items
            .filter(|(_, title, original, y, _)| {
                let hit = normalize(title).contains(&q)
                    || original.is_some_and(|o| normalize(o).contains(&q));
                hit && year.is_none_or(|want| y.is_none_or(|y| y == want))
            })
            .map(|(id, title, original, year, overview)| SearchResult {
                provider: "tmdb".into(),
                id: id.into(),
                title: title.into(),
                original_title: original.map(Into::into),
                year,
                overview: overview.map(Into::into),
            })
            .collect()
    }

    pub fn search_series(&self, query: &str, year: Option<i64>) -> Vec<SearchResult> {
        Self::search(
            self.series.iter().map(|s| {
                (
                    s.id.as_str(),
                    s.title.as_str(),
                    s.original_title.as_deref(),
                    s.year,
                    s.overview.as_deref(),
                )
            }),
            query,
            year,
        )
    }

    pub fn search_movies(&self, query: &str, year: Option<i64>) -> Vec<SearchResult> {
        Self::search(
            self.movies.iter().map(|m| {
                (
                    m.id.as_str(),
                    m.title.as_str(),
                    m.original_title.as_deref(),
                    m.year,
                    m.overview.as_deref(),
                )
            }),
            query,
            year,
        )
    }

    pub fn series(&self, id: &str) -> Option<&SeriesData> {
        self.series.iter().find(|s| s.id == id)
    }

    pub fn movie(&self, id: &str) -> Option<&MovieData> {
        self.movies.iter().find(|m| m.id == id)
    }

    pub fn find(&self, provider: &str, id: &str) -> Option<(String, String)> {
        let has = |ids: &[(String, String)]| ids.iter().any(|(p, v)| p == provider && v == id);
        self.series
            .iter()
            .find(|s| has(&s.external_ids))
            .map(|s| ("series".to_string(), s.id.clone()))
            .or_else(|| {
                self.movies
                    .iter()
                    .find(|m| has(&m.external_ids))
                    .map(|m| ("movie".to_string(), m.id.clone()))
            })
    }
}
