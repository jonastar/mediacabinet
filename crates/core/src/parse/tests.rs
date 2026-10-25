//! The filename corpus. Each case states only what it checks; everything else in
//! `Parsed` is free.

use super::*;

struct Case {
    name: &'static str,
    title: Option<&'static str>,
    year: Option<i64>,
    season: Option<i64>,
    episodes: &'static [i64],
    absolute: &'static [i64],
    episode_title: Option<&'static str>,
}

const fn case(name: &'static str) -> Case {
    Case {
        name,
        title: None,
        year: None,
        season: None,
        episodes: &[],
        absolute: &[],
        episode_title: None,
    }
}

impl Case {
    const fn title(mut self, t: &'static str) -> Self {
        self.title = Some(t);
        self
    }
    const fn year(mut self, y: i64) -> Self {
        self.year = Some(y);
        self
    }
    const fn se(mut self, s: i64, e: &'static [i64]) -> Self {
        self.season = Some(s);
        self.episodes = e;
        self
    }
    const fn abs(mut self, a: &'static [i64]) -> Self {
        self.absolute = a;
        self
    }
    const fn ep_title(mut self, t: &'static str) -> Self {
        self.episode_title = Some(t);
        self
    }
}

const EPISODES: &[Case] = &[
    // Fixture library.
    case("Night Harbor - S01E01 - Pilot.mkv")
        .title("Night Harbor")
        .se(1, &[1])
        .ep_title("Pilot"),
    case("Night.Harbor.S02E01.1080p.WEB-DL.DDP5.1.H.264-GRP.mkv")
        .title("Night Harbor")
        .se(2, &[1]),
    case("Night.Harbor.S02E02E03.1080p.WEB-DL.DDP5.1.H.264-GRP.mkv")
        .title("Night Harbor")
        .se(2, &[2, 3]),
    case("Night Harbor - S00E01 - Behind the Scenes.mkv")
        .se(0, &[1])
        .ep_title("Behind the Scenes"),
    case("Glass Orchard 1x01.mp4")
        .title("Glass Orchard")
        .se(1, &[1]),
    case("Glass Orchard 1x03 - Grafting.mp4")
        .title("Glass Orchard")
        .se(1, &[3])
        .ep_title("Grafting"),
    case("Unmatched Show S01E01.mkv")
        .title("Unmatched Show")
        .se(1, &[1]),
    case("Lantern.Street.S01E02.720p.mkv")
        .title("Lantern Street")
        .se(1, &[2]),
    case("Paper Moons - S03E10.mkv")
        .title("Paper Moons")
        .se(3, &[10]),
    case("[Kumo] Starfall Brigade - 01 [1080p][A1B2C3D4].mkv")
        .title("Starfall Brigade")
        .abs(&[1]),
    case("[Kumo] Starfall Brigade - 14 [1080p][D4E5F6A7].mkv")
        .title("Starfall Brigade")
        .abs(&[14]),
    case("[Other-Group] Starfall Brigade - 15v2 (BD 1080p HEVC).mkv")
        .title("Starfall Brigade")
        .abs(&[15]),
    // Common release styles.
    case("Show.Name.S01E01.Pilot.720p.HDTV.x264-GRP.mkv")
        .title("Show Name")
        .se(1, &[1])
        .ep_title("Pilot"),
    case("show.name.s1e5.mkv").title("show name").se(1, &[5]),
    case("Show Name S01 E02.mkv").title("Show Name").se(1, &[2]),
    case("Show Name - S01E01-E03 - Triple.mkv")
        .se(1, &[1, 2, 3])
        .ep_title("Triple"),
    case("Show.Name.S01E01-03.mkv").se(1, &[1, 2, 3]),
    case("Show Name 2x05-06.mkv")
        .title("Show Name")
        .se(2, &[5, 6]),
    case("Show Name Season 1 Episode 2.mkv")
        .title("Show Name")
        .se(1, &[2]),
    case("Doctor Who (2005) - S01E01 - Rose.mkv")
        .title("Doctor Who")
        .year(2005)
        .se(1, &[1])
        .ep_title("Rose"),
    case("Doctor.Who.2005.S01E01.mkv")
        .title("Doctor Who")
        .year(2005)
        .se(1, &[1]),
    case("Show - S01E01 - The Title (1080p BluRay x265 10bit Tigole).mkv").ep_title("The Title"),
    case("One Piece - 1071 [1080p].mkv")
        .title("One Piece")
        .abs(&[1071]),
    case("[SubsPlease] Kitsune Road S2 - 05 (1080p) [ABCD1234].mkv")
        .title("Kitsune Road")
        .se(2, &[5]),
    case("Kitsune Road - Episode 14.mkv")
        .title("Kitsune Road")
        .abs(&[14]),
    case("Kitsune Road E07.mkv").title("Kitsune Road").abs(&[7]),
    case("Kitsune Road 07 [720p].mkv")
        .title("Kitsune Road")
        .abs(&[7]),
];

#[test]
fn episodes() {
    let mut failures = Vec::new();
    for c in EPISODES {
        let p = parse(c.name);
        let mut wrong = Vec::new();
        if let Some(t) = c.title
            && p.title.as_deref() != Some(t)
        {
            wrong.push(format!("title {:?}", p.title));
        }
        if c.year.is_some() && p.year != c.year {
            wrong.push(format!("year {:?}", p.year));
        }
        if c.season.is_some() && (p.season != c.season || p.episodes != c.episodes) {
            wrong.push(format!("season {:?} episodes {:?}", p.season, p.episodes));
        }
        if !c.absolute.is_empty() && p.absolute != c.absolute {
            wrong.push(format!("absolute {:?}", p.absolute));
        }
        if let Some(t) = c.episode_title
            && p.episode_title.as_deref() != Some(t)
        {
            wrong.push(format!("episode title {:?}", p.episode_title));
        }
        if !wrong.is_empty() {
            failures.push(format!("{}: {}", c.name, wrong.join(", ")));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn daily_shows() {
    let p = parse("The.Daily.Show.2021.03.04.720p.WEB.mkv");
    assert_eq!(p.title.as_deref(), Some("The Daily Show"));
    assert_eq!(p.date.map(|d| d.to_string()).as_deref(), Some("2021-03-04"));
    assert_eq!(p.season, None);
}

#[test]
fn movies() {
    let cases: &[(&str, &str, Option<i64>)] = &[
        (
            "The Quiet Meridian (2014).mkv",
            "The Quiet Meridian",
            Some(2014),
        ),
        (
            "some.random.movie.2010.720p.BluRay.x264-NOGRP.mkv",
            "some random movie",
            Some(2010),
        ),
        ("1917 (2019).mkv", "1917", Some(2019)),
        (
            "Blade Runner 2049 (2017).mkv",
            "Blade Runner 2049",
            Some(2017),
        ),
        (
            "2001.A.Space.Odyssey.1968.1080p.mkv",
            "2001 A Space Odyssey",
            Some(1968),
        ),
        ("Heat.mkv", "Heat", None),
        (
            "Long Winter Road (1987) - cd1.mkv",
            "Long Winter Road",
            Some(1987),
        ),
    ];
    for (name, title, year) in cases {
        let p = parse(name);
        assert_eq!(
            (p.title.as_deref(), p.year),
            (Some(*title), *year),
            "{name}"
        );
        assert!(
            p.episodes.is_empty() && p.absolute.is_empty(),
            "{name}: no episode numbers"
        );
    }
}

#[test]
fn editions_parts_and_versions() {
    let p = parse("The Quiet Meridian (2014) - Director's Cut.mkv");
    assert_eq!(
        (p.title.as_deref(), p.edition.as_deref()),
        (Some("The Quiet Meridian"), Some("Director's Cut"))
    );
    assert_eq!(
        parse("Movie.Name.2010.Extended.1080p.BluRay.mkv")
            .edition
            .as_deref(),
        Some("Extended")
    );
    assert_eq!(
        parse("Movie Name (2010) {edition-Final Cut}.mkv")
            .edition
            .as_deref(),
        Some("Final Cut")
    );
    assert_eq!(parse("Long Winter Road (1987) - cd2.mkv").part, Some(2));
    assert_eq!(
        parse("Movie Name Part 2 (2010).mkv").part,
        None,
        "\"Part 2\" in a title isn't a file part"
    );
    assert_eq!(parse("Movie Name (2010) - pt1.mkv").part, Some(1));
    assert_eq!(parse("[Grp] Show - 15v2 [1080p].mkv").version, Some(2));
}

#[test]
fn technical_tags() {
    let p = parse("Halfway Coast (2001) - 2160p HDR10.mkv");
    assert_eq!(
        (p.title.as_deref(), p.year),
        (Some("Halfway Coast"), Some(2001))
    );
    assert_eq!(p.resolution.as_deref(), Some("2160p"));
    assert_eq!(
        parse("Night.Harbor.S02E01.1080p.WEB-DL.DDP5.1.H.264-GRP.mkv")
            .release_group
            .as_deref(),
        Some("GRP")
    );
    assert_eq!(
        parse("[Kumo] Starfall Brigade - 01 [1080p].mkv")
            .release_group
            .as_deref(),
        Some("Kumo")
    );
    assert_eq!(
        parse("Movie.2010.4K.mkv").resolution.as_deref(),
        Some("2160p")
    );
}

#[test]
fn extras() {
    assert_eq!(
        parse("The Quiet Meridian (2014)-trailer.mp4").extra,
        Some(Extra::Trailer)
    );
    assert_eq!(
        parse("sample-night.harbor.s01e02.mkv").extra,
        Some(Extra::Sample)
    );
    assert_eq!(
        parse("Night.Harbor.S01E02.SAMPLE.mkv").extra,
        Some(Extra::Sample)
    );
    assert_eq!(
        parse("Movie (2010) - Behind the Scenes-behindthescenes.mkv").extra,
        Some(Extra::BehindTheScenes)
    );
    assert_eq!(parse("Movie-featurette.mkv").extra, Some(Extra::Featurette));
    assert_eq!(
        parse("Sample Size (2019).mkv").extra,
        None,
        "a title word isn't a tag"
    );
}

#[test]
fn inline_ids() {
    let p = parse("Movie Name (2010) [imdbid-tt0123456].mkv");
    assert_eq!(p.ids, [("imdb".to_string(), "tt0123456".to_string())]);
    assert_eq!(p.title.as_deref(), Some("Movie Name"));
    let p = parse("Show {tmdb-1399} - S01E01.mkv");
    assert_eq!(p.ids, [("tmdb".to_string(), "1399".to_string())]);
    assert_eq!(p.title.as_deref(), Some("Show"));
}
