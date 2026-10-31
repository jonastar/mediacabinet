# Matching quirks

Real-world cases where filenames, providers and orderings disagree, and how the
matcher handles each. Add one when you handle a new case: what was seen, why it
happens, what the matcher does, and the test that pins it.

The matcher is in `crates/core/src/matcher/` (episodes in `episodes.rs`); the
filename parser in `crates/core/src/parse/`.

## Episode titles differ between providers

- **Seen:** a file named with one source's short title ("Diet") for an episode
  TMDB names in full ("The South Bronx Paradise Diet"). Similarity is low, so the
  title check marked a correct match as wrong.
- **Why:** Sonarr names files from TVDB; we use TMDB. Titles are edited
  independently.
- **Handling:** a title counts as matching if it is similar (≥ 0.8) _or_ the
  shorter appears in the longer as whole words (`score::title_within`, at least
  3 characters). Placeholder titles ("Episode 5") are never held against a file.
- **Tests:** `runtime_and_title_checks_adjust_confidence` in `episodes.rs`.

## One episode split over two files

- **Seen:** Brooklyn Nine-Nine's finale, `S08E09 - The Last Day (1)` and
  `S08E10 - The Last Day (2)`, 22 min each. TMDB has one 46-minute S08E09 and no
  E10.
- **Why:** TVDB (and so Sonarr) splits double-length episodes that TMDB keeps
  whole, or the other way round.
- **Handling:** `resolve_parts` groups part-marked files ("(1)", "Part 2", "pt2")
  by season and episode title, or by number without a title. The numbers need not
  be consecutive: two files both `S01E03` with "Part 1" and "Part 2" work too. The
  group links to the episode its numbers find with that title, else the only
  episode of that title, with `file_link.part` set. Combined runtime is checked
  against the episode. Left alone when the provider also has the halves as
  episodes, or two files claim the same part (copies). A lone part, or parts that
  don't add up, go to review with the part on the candidate.
- **Not yet:** playback doesn't chain parts; organize doesn't add a part suffix.
- **Tests:** `an_episode_split_over_two_files`, `halves_the_series_has_as_episodes`
  in `episodes.rs`; `episodes_split_over_files` in `tests/matching.rs`.

## Multi-episode files

- **Seen:** `S02E02E03`, `S01E01-E03`: one file, several episodes (the reverse of
  a split).
- **Handling:** the parser expands ranges; the file links to every item. "E01-E03"
  is a range, not two episodes.
- **Tests:** parser corpus; `tv_library` in `tests/matching.rs`.

## Absolute numbering in a season name

- **Seen:** anime released as `S01E14` where season 1 has 12 episodes: the number
  is absolute (S02E02).
- **Handling:** an episode past the end of a season is read in an absolute
  ordering, at lower confidence. When the provider has no absolute ordering, one is
  derived from aired order (`docs/decisions.md`).
- **Guard:** the absolute reading must land in a _later_ season. Without it,
  Brooklyn Nine-Nine's `S08E10` read as absolute 10, a season 1 episode.
- **Tests:** `absolute_numbers`, `past_a_season_is_not_an_earlier_absolute_number`.

## Bare numbers in a season folder

- **Seen:** `Show - 03.mkv` inside `Season 02/`.
- **Handling:** read as S02E03 from the folder. If the number also fits a different
  episode in absolute order, confidence drops below auto-linking.
- **Tests:** `absolute_numbers` in `episodes.rs`.

## Aired vs DVD (and other) orders

- **Seen:** a file numbered by DVD order (S01E13) where aired order's season 1
  ends at 12.
- **Handling:** numbers are tried in the series' matching ordering, then the
  default, then the rest; a hit outside the first gets lower confidence.
- **Tests:** `other_orderings_when_the_default_has_no_such_number`.

## Parts, years and titles in names

- "Part 2" in a movie _title_ (`Movie Name Part 2 (2010)`) isn't a file part; only
  after the year or episode marker is it one.
- "(2019)" after an episode title is a year, not part 2019; "(2)" is a part.
- **Tests:** `editions_parts_and_versions` in `parse/tests.rs`.

## Same-named series

- **Seen:** two series called "Night Harbor" (2019 and 2004), folder without a
  year or ID.
- **Handling:** when the top search results are within 0.1 of each other, none is
  confident; the folder goes to review instead of a guess.
- **Tests:** `uncertain_matches_go_to_review` in `tests/matching.rs`.
