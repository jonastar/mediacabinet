//! Layout templates: how a root's folders are organised.

use std::collections::HashMap;

use regex::Regex;
use serde::Serialize;
use utoipa::ToSchema;

pub const DEFAULT_TV: &str = "{series}/Season {season:00}/{filename}";
pub const DEFAULT_MOVIES: &str = "{movie} ({year})/{filename}";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryKind {
    Tv,
    Movies,
}

impl LibraryKind {
    pub fn parse(s: &str) -> Option<LibraryKind> {
        match s {
            "tv" => Some(LibraryKind::Tv),
            "movies" => Some(LibraryKind::Movies),
            _ => None,
        }
    }

    /// The token that marks the item folder in this kind of library.
    pub fn item_token(self) -> &'static str {
        match self {
            LibraryKind::Tv => "series",
            LibraryKind::Movies => "movie",
        }
    }

    pub fn default_template(self) -> &'static str {
        match self {
            LibraryKind::Tv => DEFAULT_TV,
            LibraryKind::Movies => DEFAULT_MOVIES,
        }
    }
}

/// Where a token may appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// Anywhere: grouping levels, the item folder or below it.
    Anywhere,
    /// Grouping levels and the item folder only.
    AboveFiles,
    /// The item folder, or below it (as a name).
    Item,
    /// Below the item folder.
    Below,
    /// The file name: the last segment, below the item folder.
    FileName,
}

const TOKENS: &[(&str, Place)] = &[
    ("year", Place::Anywhere),
    ("genre", Place::AboveFiles),
    ("letter", Place::AboveFiles),
    ("series", Place::Item),
    ("movie", Place::Item),
    ("season", Place::Below),
    ("episode", Place::Below),
    ("absolute", Place::Below),
    ("title", Place::Below),
    ("edition", Place::Below),
    ("resolution", Place::Below),
    ("filename", Place::FileName),
    ("ext", Place::FileName),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    Literal(String),
    /// A token, with a zero-padding width from `{season:00}`.
    Token {
        name: String,
        width: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub parts: Vec<Part>,
}

impl Segment {
    fn has(&self, name: &str) -> bool {
        self.parts
            .iter()
            .any(|p| matches!(p, Part::Token { name: n, .. } if n == name))
    }

    fn text(&self) -> String {
        self.parts
            .iter()
            .map(|p| match p {
                Part::Literal(s) => s.clone(),
                Part::Token { name, width: 0 } => format!("{{{name}}}"),
                Part::Token { name, width } => format!("{{{name}:{}}}", "0".repeat(*width)),
            })
            .collect()
    }

    /// A regex matching one path component, capturing each token by name.
    /// Repeated tokens (e.g. `{series}` twice) capture only the first time.
    fn regex(&self) -> Regex {
        let mut pattern = String::from("(?i)^");
        let mut seen = std::collections::HashSet::new();
        for part in &self.parts {
            match part {
                Part::Literal(s) => pattern.push_str(&regex::escape(s)),
                Part::Token { name, .. } => {
                    let body = match name.as_str() {
                        "year" => r"\d{4}",
                        "season" | "episode" | "absolute" => r"\d+",
                        "resolution" => r"\d{3,4}[pi]",
                        "letter" => r".",
                        "ext" => r"[A-Za-z0-9]+",
                        _ => r".+?",
                    };
                    if seen.insert(name.clone()) {
                        pattern.push_str(&format!("(?P<{name}>{body})"));
                    } else {
                        pattern.push_str(&format!("(?:{body})"));
                    }
                }
            }
        }
        pattern.push('$');
        Regex::new(&pattern).expect("template segments build valid regexes")
    }
}

#[derive(Debug, Clone)]
pub struct Template {
    pub segments: Vec<Segment>,
    /// Index of the segment holding `{series}` or `{movie}`.
    pub item_index: usize,
    regexes: Vec<Regex>,
}

impl PartialEq for Template {
    fn eq(&self, other: &Self) -> bool {
        self.segments == other.segments
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct TemplateError(pub String);

fn err<T>(message: impl Into<String>) -> Result<T, TemplateError> {
    Err(TemplateError(message.into()))
}

/// How a piece of a template is shown: grouping levels, the item folder, levels
/// below it, or a `/` separator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemplateRole {
    Grouping,
    Item,
    Below,
    Separator,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct TemplatePiece {
    pub text: String,
    pub role: TemplateRole,
}

impl Template {
    pub fn parse(source: &str, kind: LibraryKind) -> Result<Template, TemplateError> {
        let source = source.trim().trim_matches('/');
        if source.is_empty() {
            return err("The template is empty.");
        }
        let mut segments = Vec::new();
        for raw in source.split('/') {
            if raw.trim().is_empty() {
                return err("The template has an empty folder level (two slashes in a row).");
            }
            segments.push(parse_segment(raw)?);
        }

        let item = kind.item_token();
        let other_item = match kind {
            LibraryKind::Tv => "movie",
            LibraryKind::Movies => "series",
        };
        if segments.iter().any(|s| s.has(other_item)) {
            return err(format!(
                "{{{other_item}}} doesn't belong in this library; use {{{item}}}."
            ));
        }
        // The first level with the item token is the item folder; later uses
        // (e.g. in the file name) are just its name.
        let Some(item_index) = segments.iter().position(|s| s.has(item)) else {
            return err(format!("The template needs a {{{item}}} folder."));
        };

        let last = segments.len() - 1;
        for (i, segment) in segments.iter().enumerate() {
            for part in &segment.parts {
                let Part::Token { name, .. } = part else {
                    continue;
                };
                let place = TOKENS
                    .iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, p)| *p)
                    .expect("parse_segment checks token names");
                let ok = match place {
                    Place::Anywhere => true,
                    Place::AboveFiles => i <= item_index,
                    Place::Item => i >= item_index,
                    Place::Below => i >= item_index,
                    Place::FileName => i == last && i > item_index,
                };
                if !ok {
                    let rule = match place {
                        Place::AboveFiles => "above or in the item folder",
                        Place::Below => "in or below the item folder",
                        Place::FileName => "in the last level, below the item folder",
                        _ => "in the item folder",
                    };
                    return err(format!("{{{name}}} can only be used {rule}."));
                }
            }
        }

        let regexes = segments.iter().map(Segment::regex).collect();
        Ok(Template {
            segments,
            item_index,
            regexes,
        })
    }

    pub fn source(&self) -> String {
        self.segments
            .iter()
            .map(Segment::text)
            .collect::<Vec<_>>()
            .join("/")
    }

    /// The template split for display, each level with its role.
    pub fn pieces(&self) -> Vec<TemplatePiece> {
        let mut pieces = Vec::new();
        for (i, segment) in self.segments.iter().enumerate() {
            if i > 0 {
                pieces.push(TemplatePiece {
                    text: "/".into(),
                    role: TemplateRole::Separator,
                });
            }
            let role = match i.cmp(&self.item_index) {
                std::cmp::Ordering::Less => TemplateRole::Grouping,
                std::cmp::Ordering::Equal => TemplateRole::Item,
                std::cmp::Ordering::Greater => TemplateRole::Below,
            };
            pieces.push(TemplatePiece {
                text: segment.text(),
                role,
            });
        }
        pieces
    }

    /// Whether the last level names the file (`{filename}` or a pattern with `{ext}`).
    pub fn names_files(&self) -> bool {
        let last = &self.segments[self.segments.len() - 1];
        self.segments.len() - 1 > self.item_index && (last.has("filename") || last.has("ext"))
    }

    /// Captures from matching path component `i` against segment `i`.
    pub(crate) fn captures(&self, i: usize, component: &str) -> Option<HashMap<String, String>> {
        let regex = self.regexes.get(i)?;
        let caps = regex.captures(component)?;
        Some(
            regex
                .capture_names()
                .flatten()
                .filter_map(|name| {
                    caps.name(name)
                        .map(|m| (name.to_string(), m.as_str().to_string()))
                })
                .collect(),
        )
    }

    /// Renders segments `range` with `values`. Missing values are an error naming the token.
    pub fn render_segments(
        &self,
        range: std::ops::Range<usize>,
        values: &HashMap<&str, String>,
    ) -> Result<Vec<String>, TemplateError> {
        let mut out = Vec::new();
        for segment in &self.segments[range] {
            let mut s = String::new();
            for part in &segment.parts {
                match part {
                    Part::Literal(text) => s.push_str(text),
                    Part::Token { name, width } => {
                        let Some(value) = values.get(name.as_str()) else {
                            return err(format!("No value for {{{name}}}."));
                        };
                        if *width > 0 && value.chars().all(|c| c.is_ascii_digit()) {
                            s.push_str(&format!("{value:0>width$}"));
                        } else {
                            s.push_str(&sanitize(value));
                        }
                    }
                }
            }
            out.push(s);
        }
        Ok(out)
    }
}

/// Makes a value safe as one path component.
fn sanitize(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    cleaned.trim().trim_end_matches('.').to_string()
}

fn parse_segment(raw: &str) -> Result<Segment, TemplateError> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                let mut inner = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some('{') | None => return err(format!("Unclosed {{ in \"{raw}\".")),
                        Some(c) => inner.push(c),
                    }
                }
                let (name, format) = inner.split_once(':').unwrap_or((&inner, ""));
                let name = name.trim().to_lowercase();
                if !TOKENS.iter().any(|(n, _)| *n == name) {
                    let known: Vec<String> =
                        TOKENS.iter().map(|(n, _)| format!("{{{n}}}")).collect();
                    return err(format!(
                        "Unknown token {{{name}}}. Known tokens: {}.",
                        known.join(", ")
                    ));
                }
                if !format.chars().all(|c| c == '0') {
                    return err(format!(
                        "{{{name}:{format}}}: padding is written with zeros, e.g. {{{name}:00}}."
                    ));
                }
                if !literal.is_empty() {
                    parts.push(Part::Literal(std::mem::take(&mut literal)));
                }
                parts.push(Part::Token {
                    name,
                    width: format.len(),
                });
            }
            '}' => return err(format!("Unmatched }} in \"{raw}\".")),
            c => literal.push(c),
        }
    }
    if !literal.is_empty() {
        parts.push(Part::Literal(literal));
    }
    Ok(Segment { parts })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tv(s: &str) -> Result<Template, TemplateError> {
        Template::parse(s, LibraryKind::Tv)
    }

    #[test]
    fn parses_the_spec_examples() {
        let t = tv(DEFAULT_TV).unwrap();
        assert_eq!(t.item_index, 0);
        assert!(t.names_files());
        let t = tv("{year}/{genre}/{series}/Season {season:00}/{filename}").unwrap();
        assert_eq!(t.item_index, 2);
        assert_eq!(
            t.source(),
            "{year}/{genre}/{series}/Season {season:00}/{filename}"
        );
        let t = tv("anime/{series}/{series} - {absolute:000} [{resolution}].{ext}").unwrap();
        assert_eq!(t.item_index, 1);
        // Files anywhere below the series folder.
        assert!(!tv("{series}").unwrap().names_files());
        assert!(Template::parse(DEFAULT_MOVIES, LibraryKind::Movies).is_ok());
    }

    #[test]
    fn explains_what_is_wrong() {
        let message = |s: &str| tv(s).unwrap_err().0;
        assert!(message("{genre}/Season {season}/{filename}").contains("needs a {series}"));
        assert!(message("{seriez}/{filename}").contains("Unknown token {seriez}"));
        assert!(message("{series}/{filename}/extra").contains("last level"));
        assert!(message("Season {season}/{series}/{filename}").contains("in or below"));
        assert!(message("{series}/{genre}/{filename}").contains("above or in"));
        assert!(message("{series}//{filename}").contains("empty folder level"));
        assert!(message("{series}/Season {season:xx}").contains("padding"));
        assert!(message("{series/{filename}").contains("Unclosed"));
        assert!(message("{movie}/{filename}").contains("doesn't belong"));
    }

    #[test]
    fn pieces_carry_roles_for_colouring() {
        let t = tv("{year}/{series}/Season {season:00}/{filename}").unwrap();
        let roles: Vec<TemplateRole> = t.pieces().iter().map(|p| p.role).collect();
        use TemplateRole::*;
        assert_eq!(
            roles,
            [
                Grouping, Separator, Item, Separator, Below, Separator, Below
            ]
        );
        assert_eq!(t.pieces()[4].text, "Season {season:00}");
    }

    #[test]
    fn renders_with_padding_and_safe_names() {
        let t =
            tv("{series}/Season {season:00}/{series} - S{season:00}E{episode:00} - {title}.{ext}")
                .unwrap();
        let values = HashMap::from([
            ("series", "Who: Again?".to_string()),
            ("season", "2".to_string()),
            ("episode", "5".to_string()),
            ("title", "Floodplain".to_string()),
            ("ext", "mkv".to_string()),
        ]);
        let parts = t.render_segments(0..3, &values).unwrap();
        assert_eq!(
            parts,
            [
                "Who- Again-",
                "Season 02",
                "Who- Again- - S02E05 - Floodplain.mkv"
            ]
        );
        let missing = t.render_segments(0..3, &HashMap::new()).unwrap_err();
        assert!(missing.0.contains("{series}"));
    }
}
