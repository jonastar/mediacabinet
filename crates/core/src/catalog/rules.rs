//! Derived state that depends on an ordering: gaps and up next. Availability,
//! monitoring and wanted are SQL views (migration 0004), so every screen and the
//! API share one definition.

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
    /// From the `item_state` view's text.
    pub fn parse(s: &str) -> Availability {
        match s {
            "available" => Availability::Available,
            "missing" => Availability::Missing,
            "not_acquired" => Availability::NotAcquired,
            "upcoming" => Availability::Upcoming,
            _ => Availability::Unknown,
        }
    }

    pub fn is_unavailable(self) -> bool {
        matches!(self, Availability::Missing | Availability::NotAcquired)
    }
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
