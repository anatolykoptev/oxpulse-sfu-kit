//! Trait for per-subscriber simulcast layer selection.
//!
//! The default implementation satisfies the common case: use the subscriber's
//! explicit `desired_layer` preference, clamped to what the publisher actually
//! sends. When the `pacer` feature is enabled, the registry drives
//! `desired_layer` from BWE — the selector sees the already-adjusted value.

use crate::ids::SfuRid;

/// Decides which simulcast RID to forward to a subscriber.
pub trait LayerSelector: Send + 'static {
    /// Choose the RID to forward given:
    /// - `desired`: subscriber's expressed preference (or BWE-adjusted layer).
    /// - `active`: RIDs the publisher is currently sending (`&[]` = unknown).
    ///
    /// Must return one of the entries in `active`, or `desired` if `active` is empty.
    fn select(&self, desired: SfuRid, active: &[SfuRid]) -> SfuRid;
}

/// Default selector: use `desired`, clamped to the best available RID ≤ desired.
///
/// If `active` is empty (publisher not yet sending simulcast), returns `desired`.
/// If no active RID ≤ desired exists, returns the lowest available RID.
#[derive(Debug, Default, Clone, Copy)]
pub struct BestFitSelector;

impl LayerSelector for BestFitSelector {
    fn select(&self, desired: SfuRid, active: &[SfuRid]) -> SfuRid {
        if active.is_empty() {
            return desired;
        }
        let rank = |r: SfuRid| -> u8 {
            if r == SfuRid::LOW { 0 } else if r == SfuRid::MEDIUM { 1 } else { 2 }
        };
        let desired_rank = rank(desired);
        // Best active RID that is ≤ desired (highest rank within that bound).
        let best_below: Option<SfuRid> = active
            .iter()
            .copied()
            .filter(|&r| rank(r) <= desired_rank)
            .max_by_key(|&r| rank(r));
        best_below.unwrap_or_else(|| {
            // All active RIDs are higher than desired — pick the lowest.
            active.iter().copied().min_by_key(|&r| rank(r)).unwrap_or(desired)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_active_returns_desired() {
        assert_eq!(BestFitSelector.select(SfuRid::MEDIUM, &[]), SfuRid::MEDIUM);
    }

    #[test]
    fn selects_matching_layer() {
        let active = [SfuRid::LOW, SfuRid::MEDIUM, SfuRid::HIGH];
        assert_eq!(BestFitSelector.select(SfuRid::MEDIUM, &active), SfuRid::MEDIUM);
    }

    #[test]
    fn clamps_to_best_below_desired() {
        // Publisher only sends q and h; consumer wants f → gets h.
        let active = [SfuRid::LOW, SfuRid::MEDIUM];
        assert_eq!(BestFitSelector.select(SfuRid::HIGH, &active), SfuRid::MEDIUM);
    }

    #[test]
    fn falls_back_to_lowest_when_all_above() {
        // Publisher only sends h and f; consumer wants q → gets h (lowest available).
        let active = [SfuRid::MEDIUM, SfuRid::HIGH];
        assert_eq!(BestFitSelector.select(SfuRid::LOW, &active), SfuRid::MEDIUM);
    }
}
