//! Metadata providers, and importing what they return into the library model.
//!
//! Each series or movie has one primary provider whose episode list is canonical;
//! other providers are cross-references.

pub mod fake;
pub mod import;
pub mod model;
pub mod omdb;
pub mod refresh;
pub mod tmdb;

use std::sync::Arc;

pub use model::*;

#[derive(Debug, Clone, thiserror::Error)]
pub enum MetadataError {
    #[error("not found at the provider")]
    NotFound,
    /// Network trouble, rate limits or provider outages: worth retrying later.
    #[error("provider unreachable: {0}")]
    Unreachable(String),
    #[error("provider refused the credentials: {0}")]
    Auth(String),
    #[error("unexpected provider response: {0}")]
    Invalid(String),
    #[error("no metadata provider is configured; set CABINET_TMDB_KEY")]
    NotConfigured,
}

/// The configured provider. An enum rather than a trait object, so its async
/// methods need no boxing.
#[derive(Clone)]
pub enum Provider {
    Tmdb(tmdb::Tmdb),
    /// An offline catalog, for tests and trying things out without an API key.
    Fake(Arc<fake::Catalog>),
    None,
}

impl Provider {
    /// The provider's name as stored in `external_id.provider`.
    pub fn name(&self) -> &'static str {
        match self {
            Provider::Tmdb(_) | Provider::Fake(_) => tmdb::PROVIDER,
            Provider::None => "none",
        }
    }

    pub fn is_configured(&self) -> bool {
        !matches!(self, Provider::None)
    }

    pub async fn search_series(
        &self,
        query: &str,
        year: Option<i64>,
    ) -> Result<Vec<SearchResult>, MetadataError> {
        match self {
            Provider::Tmdb(t) => t.search_series(query, year).await,
            Provider::Fake(c) => Ok(c.search_series(query, year)),
            Provider::None => Err(MetadataError::NotConfigured),
        }
    }

    pub async fn search_movies(
        &self,
        query: &str,
        year: Option<i64>,
    ) -> Result<Vec<SearchResult>, MetadataError> {
        match self {
            Provider::Tmdb(t) => t.search_movies(query, year).await,
            Provider::Fake(c) => Ok(c.search_movies(query, year)),
            Provider::None => Err(MetadataError::NotConfigured),
        }
    }

    pub async fn series(&self, id: &str) -> Result<SeriesData, MetadataError> {
        match self {
            Provider::Tmdb(t) => t.series(id).await,
            Provider::Fake(c) => c.series(id).cloned().ok_or(MetadataError::NotFound),
            Provider::None => Err(MetadataError::NotConfigured),
        }
    }

    pub async fn movie(&self, id: &str) -> Result<MovieData, MetadataError> {
        match self {
            Provider::Tmdb(t) => t.movie(id).await,
            Provider::Fake(c) => c.movie(id).cloned().ok_or(MetadataError::NotFound),
            Provider::None => Err(MetadataError::NotConfigured),
        }
    }

    pub async fn person(&self, id: &str) -> Result<PersonData, MetadataError> {
        match self {
            Provider::Tmdb(t) => t.person(id).await,
            // The offline catalog only has people as credits.
            Provider::Fake(_) => Err(MetadataError::NotFound),
            Provider::None => Err(MetadataError::NotConfigured),
        }
    }

    /// Translates another provider's ID (e.g. `tvdb`, `imdb`) into this provider's.
    /// Returns ("series" | "movie", id).
    pub async fn find(
        &self,
        provider: &str,
        id: &str,
    ) -> Result<Option<(String, String)>, MetadataError> {
        match self {
            Provider::Tmdb(t) => t.find(provider, id).await,
            Provider::Fake(c) => Ok(c.find(provider, id)),
            Provider::None => Err(MetadataError::NotConfigured),
        }
    }
}
