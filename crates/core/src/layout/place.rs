//! Where a new file goes. From the spec's placement rules:
//!
//! - A series' folder is resolved once and stored; it's never recomputed from
//!   metadata, so a genre change never moves files.
//! - Managed (an import that renames by the layout, and Organise): placed and
//!   named by the template.
//! - Adaptive (an import that keeps names): next to its siblings if their season
//!   folder exists, otherwise into the series' stored folder by the template,
//!   keeping the original name.
//! - Read-only: never written; the library's root for new items takes it instead.

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
            note: "Not written here. It goes to the folder for new items, and the series gets a second location there."
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
                note: if template.names_files() {
                    format!("Renamed by the template; the series folder is {stored}.")
                } else {
                    format!(
                        "Placed by the template, which keeps file names; the series folder is {stored}."
                    )
                },
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

/// A sample file placed by a template, to show what a layout does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct LayoutExample {
    /// The name the file arrives with.
    pub filename: String,
    /// The item's folder, relative to the root.
    pub folder: Option<String>,
    /// Where Organise or a renaming import puts it, relative to the root.
    pub path: Option<String>,
    /// Why it can't be placed, e.g. a token this file has no value for.
    pub problem: Option<String>,
}

/// Fixed sample files placed by `template` as a new item, renamed to the layout.
/// The second sample of each kind lacks some values (resolution, edition), so a
/// template that needs them shows which files Organise would skip.
pub fn examples(template: &Template, kind: super::LibraryKind) -> Vec<LayoutExample> {
    use super::LibraryKind;
    let samples: Vec<(&str, Vec<(&str, &str)>)> = match kind {
        LibraryKind::Tv => vec![
            (
                "Lowland.S02E01.1080p.WEB-DL.mkv",
                vec![
                    ("series", "Lowland"),
                    ("year", "2014"),
                    ("genre", "Drama"),
                    ("season", "2"),
                    ("episode", "1"),
                    ("absolute", "11"),
                    ("title", "Floodplain"),
                    ("resolution", "1080p"),
                ],
            ),
            (
                "harbour.lights.103.mkv",
                vec![
                    ("series", "Harbour Lights"),
                    ("year", "2019"),
                    ("genre", "Comedy"),
                    ("season", "1"),
                    ("episode", "3"),
                    ("absolute", "3"),
                    ("title", "The Lighthouse"),
                ],
            ),
        ],
        LibraryKind::Movies => vec![
            (
                "Arrival.2016.2160p.UHD.BluRay.mkv",
                vec![
                    ("movie", "Arrival"),
                    ("year", "2016"),
                    ("genre", "Science Fiction"),
                    ("resolution", "2160p"),
                ],
            ),
            (
                "The Third Man (1949) Criterion.mp4",
                vec![
                    ("movie", "The Third Man"),
                    ("year", "1949"),
                    ("genre", "Thriller"),
                    ("edition", "Criterion"),
                ],
            ),
        ],
        LibraryKind::Folders => Vec::new(),
    };
    samples
        .into_iter()
        .map(|(filename, values)| {
            let incoming = Incoming {
                filename,
                values: values
                    .into_iter()
                    .map(|(k, v)| (k, v.to_string()))
                    .collect(),
            };
            let folder = template
                .render_segments(0..template.item_index + 1, &incoming.values)
                .map(|parts| parts.join("/"));
            let placed = place(WritePolicy::Managed, template, None, None, &incoming);
            match (folder, placed) {
                (Ok(folder), Ok(placed)) => LayoutExample {
                    filename: filename.to_string(),
                    folder: Some(folder),
                    path: placed.path,
                    problem: None,
                },
                (folder, placed) => LayoutExample {
                    filename: filename.to_string(),
                    folder: folder.as_ref().ok().cloned(),
                    path: None,
                    problem: placed.err().or(folder.err()).map(|e| e.0),
                },
            }
        })
        .collect()
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

    #[test]
    fn examples_show_where_sample_files_go() {
        let t = template(
            "{series} ({year})/Season {season:00}/{series} - S{season:00}E{episode:00} [{resolution}].{ext}",
        );
        let shown = examples(&t, LibraryKind::Tv);
        assert_eq!(shown[0].folder.as_deref(), Some("Lowland (2014)"));
        assert_eq!(
            shown[0].path.as_deref(),
            Some("Lowland (2014)/Season 02/Lowland - S02E01 [1080p].mkv")
        );
        // The second sample has no resolution in its name.
        assert_eq!(shown[1].folder.as_deref(), Some("Harbour Lights (2019)"));
        assert_eq!(shown[1].path, None);
        assert!(
            shown[1]
                .problem
                .as_deref()
                .unwrap()
                .contains("{resolution}")
        );

        let kept = examples(&template("{series}"), LibraryKind::Tv);
        assert_eq!(
            kept[1].path.as_deref(),
            Some("Harbour Lights/harbour.lights.103.mkv")
        );
    }
}
