//! Opaque SFU-owned handle over a str0m `Rtc` state machine.
//!
//! Downstream consumers construct an [`SfuRtc`] via [`SfuRtcBuilder`] and
//! hand it to [`Client::new`][crate::Client::new] (Task 7 migration). The
//! underlying str0m type is intentionally hidden. For advanced configuration
//! not covered by the builder, drop down to [`crate::raw`] — note that
//! escape hatch is **not** covered by this crate's semver guarantee.

use std::time::Instant;

/// An opaque WebRTC peer connection state machine.
///
/// Construct via [`SfuRtcBuilder`]. Pass to [`Client::new`][crate::Client::new].
// Field consumed by Client::new in Task 7; suppress dead_code until then.
#[allow(dead_code)]
pub struct SfuRtc(pub(crate) str0m::Rtc);

impl std::fmt::Debug for SfuRtc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SfuRtc").finish_non_exhaustive()
    }
}

impl SfuRtc {
    /// Wrap a raw str0m `Rtc` obtained via [`crate::raw`].
    ///
    /// This constructor is the supported bridge for advanced use that builds
    /// an `Rtc` with options [`SfuRtcBuilder`] does not expose. Callers who
    /// use it opt into str0m's semver cycle.
    pub fn from_raw(rtc: str0m::Rtc) -> Self {
        Self(rtc)
    }
}

/// Builder for [`SfuRtc`] exposing a curated subset of str0m configuration.
///
/// For advanced configuration (custom codec formats, non-standard extensions,
/// custom schedulers), drop down to [`crate::raw::rtc_config`] and wrap the
/// result via [`SfuRtc::from_raw`].
///
/// # Example
///
/// ```no_run
/// use oxpulse_sfu_kit::SfuRtcBuilder;
/// let rtc = SfuRtcBuilder::new().enable_bwe(2_000_000).build();
/// ```
#[derive(Debug)]
pub struct SfuRtcBuilder {
    inner: str0m::RtcConfig,
}

impl Default for SfuRtcBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SfuRtcBuilder {
    /// Start a new builder with str0m defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: str0m::RtcConfig::default(),
        }
    }

    /// Enable egress bandwidth estimation (TWCC + GoogCC) with an initial
    /// estimate in bits per second.
    ///
    /// Pass a conservative value (e.g. 500_000 for 500 kbps) — GoogCC will
    /// ramp from there based on TWCC feedback.
    #[must_use]
    pub fn enable_bwe(mut self, initial_bitrate_bps: u64) -> Self {
        self.inner = self
            .inner
            .enable_bwe(Some(str0m::bwe::Bitrate::bps(initial_bitrate_bps)));
        self
    }

    /// Finish and produce an [`SfuRtc`].
    #[must_use]
    pub fn build(self) -> SfuRtc {
        SfuRtc(self.inner.build(Instant::now()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_produces_sfu_rtc() {
        let rtc = SfuRtcBuilder::new().build();
        // Smoke: just ensure build() returns an SfuRtc and Debug works.
        let dbg = format!("{rtc:?}");
        assert!(dbg.contains("SfuRtc"), "Debug output should mention SfuRtc");
    }

    #[test]
    fn from_raw_preserves_rtc() {
        let raw = str0m::Rtc::new(Instant::now());
        let sfu = SfuRtc::from_raw(raw);
        let dbg = format!("{sfu:?}");
        assert!(dbg.contains("SfuRtc"));
    }
}
