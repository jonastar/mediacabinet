//! Where a new file goes, by write policy. From the spec's placement rules:
//!
//! - A series' folder is resolved once and stored; it's never recomputed from
//!   metadata, so a genre change never moves files.
//! - Managed: placed and named by the template.
//! - Adaptive: next to its siblings if their season folder exists, otherwise into
//!   the series' stored folder by the template, keeping the original name.
//! - Read-only: never written; the import target takes it instead.

use std::collections::HashMap;

use serde::Serialize;
use utoipa::ToSchema;

use super::template::{Template, TemplateError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WritePolicy {
    Managed,
    Adaptive,
    ReadOnly,
}

impl WritePolicy {
    pub fn parse(s: &str) -> Option<WritePolicy> {
        match s {
            "managed" => Some(WritePolicy::Managed),
            "adaptive" => Some(WritePolicy::Adaptive),
            "read_only" => Some(WritePolicy::ReadOnly),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            WritePolicy::Managed => "managed",
            WritePolicy::Adaptive => "adaptive",
            WritePolicy::ReadOnly => "read_only",
        }
    }
}

/// An incoming file.
#[derive(Debug, Clone)]
pub struct Incoming<'a> {
    /// The name it arrived with.
    pub filename: &'a str,
    /// Token values: series, year, genre, season, episode, absolute, title, resolution…
    pub values: HashMap<&'a str, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct Placement {
    /// Relative to the root; None when nothing is written to this root.
    pub path: Option<String>,
    pub note: String,
}

/// `location`: the series' stored folder on this root, if it has one.
/// `sibling_folder`: an existing folder (relative to the root) holding other files
/// of the same season, if any.
pub fn place(
    policy: WritePolicy,
    template: &Template,
    location: Option<&str>,
    sibling_folder: Option<&str>,
    incoming: &Incoming,
) -> Result<Placement, TemplateError> {
    let mut values = incoming.values.clone();
    values.insert("filename", incoming.filename.to_string());
    if let Some((_, ext)) = incoming.filename.rsplit_once('.') {
        values.entry("ext").or_insert_with(|| ext.to_string());
    }
    if let Some(series) = values
        .get("series")
        .or_else(|| values.get("movie"))
        .cloned()
        && let Some(first) = series.chars().find(|c| c.is_alphanumeric())
    {
        values
            .entry("letter")
            .or_insert_with(|| first.to_uppercase().to_string());
    }

    let item = template.item_index;
    let folder = match location {
        Some(location) => location.to_string(),
        None => template.render_segments(0..item + 1, &values)?.join("/"),
    };
    let stored = if location.is_some() {
        "from the stored location, not today's metadata"
    } else {
        "resolved now from the template and stored, so it never moves later"
    };

    match policy {
        WritePolicy::ReadOnly => Ok(Placement {
            path: None,
            note: "Not written here. It goes to the import target, and the series gets a second location there."
                .into(),
        }),
        WritePolicy::Adaptive => {
            if let Some(sibling) = sibling_folder {
                return Ok(Placement {
                    path: Some(format!("{sibling}/{}", incoming.filename)),
                    note: "Kept as the downloader named it, next to the existing files of that season.".into(),
                });
            }
            let mut parts = vec![folder];
            parts.extend(below_folders(template, &values)?);
            parts.push(incoming.filename.to_string());
            Ok(Placement {
                path: Some(parts.join("/")),
                note: format!("Kept as the downloader named it, in a folder made by the template; the series folder is {stored}."),
            })
        }
        WritePolicy::Managed => {
            let mut parts = vec![folder];
            parts.extend(below_folders(template, &values)?);
            let name = if template.names_files() {
                let last = template.segments.len() - 1;
                template.render_segments(last..last + 1, &values)?.remove(0)
            } else {
                incoming.filename.to_string()
            };
            parts.push(name);
            Ok(Placement {
                path: Some(parts.join("/")),
                note: format!("Renamed by the template; the series folder is {stored}."),
            })
        }
    }
}

/// The folders between the item folder and the file name.
fn below_folders(
    template: &Template,
    values: &HashMap<&str, String>,
) -> Result<Vec<String>, TemplateError> {
    let start = template.item_index + 1;
    let end = if template.names_files() {
        template.segments.len() - 1
    } else {
        template.segments.len()
    };
    template.render_segments(start..end.max(start), values)
}

#[cfg(test)]
mod tests {
    use super::super::template::LibraryKind;
    use super::*;

    fn lowland() -> Incoming<'static> {
        Incoming {
            filename: "Lowland.S02E01.mkv",
            values: HashMap::from([
                ("series", "Lowland".to_string()),
                ("year", "2014".to_string()),
                ("genre", "Drama".to_string()),
                ("season", "2".to_string()),
                ("episode", "1".to_string()),
                ("title", "Floodplain".to_string()),
            ]),
        }
    }

    fn template(s: &str) -> Template {
        Template::parse(s, LibraryKind::Tv).unwrap()
    }

    #[test]
    fn the_mockups_example() {
        let t = template(
            "{year}/{genre}/{series}/Season {season:00}/{series} - S{season:00}E{episode:00} - {title}.{ext}",
        );
        // The stored location says "drama"; today's metadata says "Drama". The stored one wins.
        let location = Some("2014/drama/Lowland");
        let sibling = Some("2014/drama/Lowland/Season 02");

        let adaptive = place(WritePolicy::Adaptive, &t, location, sibling, &lowland()).unwrap();
        assert_eq!(
            adaptive.path.as_deref(),
            Some("2014/drama/Lowland/Season 02/Lowland.S02E01.mkv")
        );
        let managed = place(WritePolicy::Managed, &t, location, sibling, &lowland()).unwrap();
        assert_eq!(
            managed.path.as_deref(),
            Some("2014/drama/Lowland/Season 02/Lowland - S02E01 - Floodplain.mkv")
        );
        let read_only = place(WritePolicy::ReadOnly, &t, location, sibling, &lowland()).unwrap();
        assert_eq!(read_only.path, None);
    }

    #[test]
    fn adaptive_without_siblings_uses_the_template_folders() {
        let t = template("{series}/Season {season:00}/{filename}");
        let p = place(
            WritePolicy::Adaptive,
            &t,
            Some("Lowland (2014)"),
            None,
            &lowland(),
        )
        .unwrap();
        assert_eq!(
            p.path.as_deref(),
            Some("Lowland (2014)/Season 02/Lowland.S02E01.mkv")
        );
    }

    #[test]
    fn a_new_series_folder_comes_from_the_template() {
        let t = template("{letter}/{series}/Season {season:00}/{filename}");
        let p = place(WritePolicy::Managed, &t, None, None, &lowland()).unwrap();
        assert_eq!(
            p.path.as_deref(),
            Some("L/Lowland/Season 02/Lowland.S02E01.mkv")
        );
        assert!(p.note.contains("resolved now"));
    }
}
