//! Integration tests verifying that str0m interop conversions are accessible
//! from outside the crate (i.e., genuinely `pub`, not `pub(crate)`).
//!
//! This file is compiled as a separate crate (under `tests/`) and therefore
//! cannot see `pub(crate)` items. Any `From` impl exercised here is part of
//! the stable public API.

use oxpulse_sfu_kit::{SfuMid, SfuPt, SfuRid};

/// `SfuRid::LOW/MEDIUM/HIGH` are `pub const` accessible outside the crate.
#[test]
fn sfu_rid_constants_are_pub() {
    let _ = SfuRid::LOW;
    let _ = SfuRid::MEDIUM;
    let _ = SfuRid::HIGH;
}

/// `From<SfuRid> for str0m::media::Rid` must be callable from outside the crate.
/// This is the primary interop path for `PacerAction::ChangeLayer` consumers.
#[test]
fn sfu_rid_into_str0m_rid() {
    let sfu = SfuRid::LOW;
    let raw: str0m::media::Rid = sfu.into();
    // Roundtrip: back to SfuRid via the reverse From.
    let back: SfuRid = raw.into();
    assert_eq!(back, SfuRid::LOW);
}

/// `From<str0m::media::Rid> for SfuRid` must be callable from outside the crate.
#[test]
fn str0m_rid_into_sfu_rid() {
    let raw = str0m::media::Rid::from("q");
    let sfu: SfuRid = raw.into();
    assert_eq!(sfu, SfuRid::LOW);
}

/// All three layer constants survive a str0m roundtrip.
#[test]
fn all_layer_constants_roundtrip() {
    for (expected, label) in [
        (SfuRid::LOW, "LOW"),
        (SfuRid::MEDIUM, "MEDIUM"),
        (SfuRid::HIGH, "HIGH"),
    ] {
        let raw: str0m::media::Rid = expected.into();
        let back: SfuRid = raw.into();
        assert_eq!(back, expected, "{label} did not roundtrip");
    }
}

/// `From<SfuMid> for str0m::media::Mid` is accessible outside the crate.
#[test]
fn sfu_mid_into_str0m_mid() {
    let raw = str0m::media::Mid::from("1");
    let sfu: SfuMid = raw.into();
    let back: str0m::media::Mid = sfu.into();
    assert_eq!(back, raw);
}

/// `From<SfuPt> for str0m::media::Pt` is accessible outside the crate.
#[test]
fn sfu_pt_into_str0m_pt() {
    let raw = str0m::media::Pt::from(96u8);
    let sfu: SfuPt = raw.into();
    let back: str0m::media::Pt = sfu.into();
    assert_eq!(back, raw);
}

/// `PacerAction::ChangeLayer` can be consumed with `From` without the
/// `sfu_rid_to_rid` workaround. This pattern is the primary driver for
/// the v0.11.1 API gap fix.
#[cfg(feature = "pacer")]
#[test]
fn pacer_action_change_layer_converts_via_from() {
    use oxpulse_sfu_kit::bwe::{PacerAction, SubscriberPacer};

    let mut pacer = SubscriberPacer::new();
    // Feed enough ticks above MEDIUM threshold to trigger ChangeLayer.
    let high_bps = 1_000_000u64; // above MEDIUM_MIN_BPS
    let mut action = PacerAction::NoChange;
    // upgrade_streak = 3 by default; feed 4 ticks to be safe.
    for _ in 0..4 {
        action = pacer.update(high_bps);
    }
    if let PacerAction::ChangeLayer(sfu_rid) = action {
        // This is the line that previously required the workaround.
        let _str0m_rid: str0m::media::Rid = sfu_rid.into();
    }
    // Even NoChange is fine here — we just need this to compile.
}
