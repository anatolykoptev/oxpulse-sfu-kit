//! DataChannel builder methods on [`Client`].
//!
//! `with_extra_dc` is the generic primitive; `with_chat_dcs` and
//! `with_voice_dc` are thin convenience shims.

use super::Client;
use crate::dc::ChannelConfig;

impl Client {
    /// Register an additional DataChannel to open during SDP negotiation.
    ///
    /// The `label`, `id`, and channel `cfg` are stored in [`Client::extra_dcs`].
    /// The application signalling layer reads these during offer/answer and
    /// calls `Rtc::open_stream(id, str0m_channel_config)` for each entry.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use oxpulse_sfu_kit::{ChannelConfig, Client, SfuRtcBuilder, SfuMetrics};
    /// use std::sync::Arc;
    ///
    /// let rtc = SfuRtcBuilder::new().build();
    /// let client = Client::new(rtc, Arc::new(SfuMetrics::new_default()))
    ///     .with_extra_dc("telemetry", 20, ChannelConfig::reliable_ordered());
    /// ```
    #[must_use]
    pub fn with_extra_dc(mut self, label: &str, id: u16, cfg: ChannelConfig) -> Self {
        debug_assert!(
            !self.extra_dcs.iter().any(|c| c.id() == id),
            "DC id {} already registered (existing labels: {:?})",
            id,
            self.extra_dcs.iter().map(|c| c.label()).collect::<Vec<_>>()
        );
        debug_assert!(
            cfg.max_packet_lifetime_ms.is_none() || cfg.max_retransmits.is_none(),
            "ChannelConfig invariant violated: max_packet_lifetime_ms and max_retransmits \
             cannot both be Some (label={label}, id={id})"
        );
        let mut entry = cfg;
        entry.id = id;
        entry.label = label.to_string();
        self.extra_dcs.push(entry);
        self
    }

    /// Register the standard OxPulse chat DataChannels (id=4 and id=5).
    ///
    /// - `"chat-data"` (id=4): reliable ordered — text messages.
    /// - `"chat-ctrl"` (id=5): unreliable, 0 retransmits — typing / presence signals.
    ///
    /// This is a thin shim over [`with_extra_dc`][Self::with_extra_dc]; no
    /// behaviour change relative to the pre-0.10.0 implementation.
    #[must_use]
    pub fn with_chat_dcs(self) -> Self {
        self.with_extra_dc("chat-data", 4, ChannelConfig::reliable_ordered())
            .with_extra_dc("chat-ctrl", 5, ChannelConfig::unreliable_max_retransmits(0))
    }

    /// Register the Phase 8 voice DataChannel (id=6).
    ///
    /// Uses unordered delivery with `Reliability::MaxPacketLifetime { lifetime: max_pkt_lifetime_ms }`.
    /// For voice control signals, 200 ms is the recommended lifetime — packets older
    /// than that are useless and should be discarded.
    #[must_use]
    pub fn with_voice_dc(self, max_pkt_lifetime_ms: u32) -> Self {
        self.with_extra_dc(
            "voice",
            6,
            ChannelConfig::unreliable_max_lifetime(max_pkt_lifetime_ms),
        )
    }
}

// ── Test seams ─────────────────────────────────────────────────────────────

#[cfg(any(test, feature = "test-utils"))]
impl Client {
    /// Build a `Client` for integration tests without real ICE/DTLS setup.
    ///
    /// Identical to [`crate::client::test_seed::new_client`] but available as an associated
    /// function on `Client` so tests can call `Client::new_for_test()` without
    /// importing the internal module.
    #[must_use]
    pub fn new_for_test() -> Self {
        crate::client::test_seed::new_client(crate::propagate::ClientId(u64::MAX))
    }

    /// Look up the [`ChannelConfig`] registered under `label` via `with_extra_dc`.
    ///
    /// Returns `None` if no DC with that label has been registered.
    #[must_use]
    pub fn dc_config_for(&self, label: &str) -> Option<&ChannelConfig> {
        self.extra_dcs.iter().find(|dc| dc.label() == label)
    }

    /// Number of DataChannels registered via `with_extra_dc` (and its shims).
    #[must_use]
    pub fn dc_count(&self) -> usize {
        self.extra_dcs.len()
    }
}
