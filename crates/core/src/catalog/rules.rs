//! Derived state, from the spec's "Availability, wanted and status" rules. Pure
//! functions so the rules are tested on their own.

use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    /// At least one linked file is present.
    Available,
    /// Had a linked file; none present, and at least one is missing.
    Missing,
    /// No present or missing files, and it has aired.
    NotAcquired,
    /// Airs in the future.
    Upcoming,
    /// No air date.
    Unknown,
}

impl Availability {
    pub fn is_unavailable(self) -> bool {
        matches!(self, Availability::Missing | Availability::NotAcquired)
    }
}

/// `present` and `missing` count linked files by status; removed and ignored don't count.
pub fn availability(present: i64, missing: i64, airs_at: Option<i64>, now: i64) -> Availability {
    if present > 0 {
        Availability::Available
    } else if missing > 0 {
        Availability::Missing
    } else {
        match airs_at {
            None => Availability::Unknown,
            Some(t) if t > now => Availability::Upcoming,
            Some(_) => Availability::NotAcquired,
        }
    }
}

/// An episode's own setting wins; otherwise it inherits the series'. Unset means not monitored.
pub fn monitored(episode: Option<bool>, series: Option<bool>) -> bool {
    episode.or(series).unwrap_or(false)
}

/// Monitored and not available. Quality cutoffs come with quality profiles.
pub fn wanted(monitored: bool, availability: Availability) -> bool {
    monitored && availability.is_unavailable()
}

/// For each entry, whether it's a gap: unavailable, with an available entry after
/// it. Entries with `counts` false (specials) are never gaps and don't make
/// earlier entries gaps either.
pub fn gaps(entries: &[Availability], counts: &[bool]) -> Vec<bool> {
    let mut later_available = false;
    let mut gaps = vec![false; entries.len()];
    for (i, a) in entries.iter().enumerate().rev() {
        if !counts.get(i).copied().unwrap_or(true) {
            continue;
        }
        gaps[i] = a.is_unavailable() && later_available;
        later_available |= *a == Availability::Available;
    }
    gaps
}

/// The first available entry the user hasn't finished.
pub fn up_next(entries: &[(Availability, bool)]) -> Option<usize> {
    entries
        .iter()
        .position(|(a, played)| *a == Availability::Available && !played)
}

#[cfg(test)]
mod tests {
    use super::Availability::*;
    use super::*;

    #[test]
    fn availability_follows_the_spec() {
        let now = 1000;
        assert_eq!(availability(1, 1, Some(10), now), Available);
        assert_eq!(availability(0, 1, Some(10), now), Missing);
        // A missing file wins over the air date: the system lost something.
        assert_eq!(availability(0, 1, Some(5000), now), Missing);
        assert_eq!(availability(0, 0, Some(10), now), NotAcquired);
        assert_eq!(availability(0, 0, Some(5000), now), Upcoming);
        assert_eq!(availability(0, 0, None, now), Unknown);
    }

    #[test]
    fn monitoring_inherits_from_the_series() {
        assert!(monitored(None, Some(true)));
        assert!(!monitored(Some(false), Some(true)));
        assert!(monitored(Some(true), None));
        assert!(!monitored(None, None));
        assert!(wanted(true, Missing) && wanted(true, NotAcquired));
        assert!(!wanted(true, Upcoming) && !wanted(false, Missing));
    }

    #[test]
    fn gaps_need_an_available_entry_after_them() {
        let entries = [
            Available,
            NotAcquired,
            Missing,
            Available,
            NotAcquired,
            Upcoming,
        ];
        assert_eq!(
            gaps(&entries, &[true; 6]),
            [false, true, true, false, false, false]
        );
        // An available special at the end doesn't turn missing episodes into gaps.
        let with_special = [Available, NotAcquired, Available];
        assert_eq!(
            gaps(&with_special, &[true, true, false]),
            [false, false, false]
        );
    }

    #[test]
    fn up_next_is_the_first_unplayed_available_entry() {
        let entries = [
            (Available, true),
            (NotAcquired, false),
            (Available, false),
            (Available, false),
        ];
        assert_eq!(up_next(&entries), Some(2));
        assert_eq!(up_next(&[(Available, true)]), None);
    }
}
