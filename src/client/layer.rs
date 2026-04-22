//! Per-subscriber simulcast layer preference.
//!
//! str0m's `Rid` type is an opaque 8-byte string id.
//! LiveKit's convention — adopted by mediasoup and Jitsi — is `"q"` (lowest),
//! `"h"` (mid), `"f"` (full). We build the three values as `const` so they
//! cost nothing at runtime and match byte-for-byte with `Rid::from("q"|"h"|"f")`.

use crate::ids::SfuRid;
use crate::media::SfuMediaPayload;

/// LiveKit low-resolution simulcast layer (`q`).
pub const LOW: SfuRid = SfuRid::LOW;
/// LiveKit mid-resolution simulcast layer (`h`).
pub const MEDIUM: SfuRid = SfuRid::MEDIUM;
/// LiveKit full-resolution simulcast layer (`f`).
pub const HIGH: SfuRid = SfuRid::HIGH;

/// Decide whether `data` should be forwarded to a subscriber whose desired
/// layer is `desired`.
///
/// Rules:
/// - `data.rid() == None` — non-simulcast publisher. Forward unconditionally.
/// - `data.rid() == Some(x)` — forward only if `x == desired`.
pub(crate) fn matches(desired: SfuRid, data: &SfuMediaPayload) -> bool {
    match data.rid() {
        None => true,
        Some(rid) => rid == desired,
    }
}

#[cfg(test)]
mod tests {
    use str0m::media::Rid;

    use super::*;

    #[test]
    fn const_matches_from_str() {
        // Invariant: our `const` SfuRids must be byte-identical to the value
        // produced by str0m's `From<&str>` impl, otherwise `Eq` silently
        // breaks the whole forwarder filter.
        assert_eq!(LOW.to_str0m(), Rid::from("q"));
        assert_eq!(MEDIUM.to_str0m(), Rid::from("h"));
        assert_eq!(HIGH.to_str0m(), Rid::from("f"));
    }
}
