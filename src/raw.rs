//! Escape hatch for advanced str0m access.
//!
//! # Semver
//!
//! Everything in this module directly re-exports [`str0m`] and is **not**
//! covered by oxpulse-sfu-kit's semver guarantee. Minor bumps of str0m may
//! break code that imports from `raw` — even when our own `MAJOR.MINOR`
//! does not change.
//!
//! # When to use
//!
//! - Building an [`SfuRtc`][crate::SfuRtc] with options [`SfuRtcBuilder`][crate::SfuRtcBuilder]
//!   does not expose (custom codec extensions, non-standard RTP extensions,
//!   advanced ICE policies).
//! - Interop with other crates that speak str0m types directly.
//! - Low-level debugging.

/// The underlying str0m `Rtc`. Semver-exempt.
pub use str0m::Rtc as RawRtc;
/// The underlying str0m `RtcConfig` builder. Semver-exempt.
pub use str0m::RtcConfig as RawRtcConfig;

/// Start a raw str0m `RtcConfig` for advanced configuration.
///
/// Wrap the finished `Rtc` via [`crate::SfuRtc::from_raw`] to feed it into a
/// [`Client`][crate::Client].
///
/// # Semver
///
/// The returned type is str0m's own builder. Using it opts into str0m's
/// pre-1.0 semver cycle; minor str0m bumps may break downstream code that
/// calls this function.
pub fn rtc_config() -> RawRtcConfig {
    RawRtcConfig::default()
}
