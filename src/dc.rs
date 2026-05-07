//! DataChannel configuration wrappers.
//!
//! Provides a thin, semver-stable façade over [`str0m::channel::ChannelConfig`]
//! and [`str0m::channel::Reliability`] so callers never import from `str0m`
//! directly for DataChannel configuration.
//!
//! # Usage
//!
//! ```rust
//! use oxpulse_sfu_kit::ChannelConfig;
//!
//! let chat   = ChannelConfig::reliable_ordered();
//! let ctrl   = ChannelConfig::unreliable_max_retransmits(0);
//! let voice  = ChannelConfig::unreliable_max_lifetime(200);
//! ```

/// Stable configuration descriptor for a DataChannel pre-registration.
///
/// One `ChannelConfig` is pushed into [`Client::extra_dcs`] by every
/// `with_extra_dc` / `with_chat_dcs` / `with_voice_dc` call.
/// The SFU application (e.g. `partner-edge`) reads these out during
/// offer/answer negotiation and passes them to `Rtc::open_stream`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelConfig {
    /// SCTP stream identifier. Must be unique per peer-connection.
    pub id: u16,
    /// Human-readable channel label (e.g. `"chat-data"`, `"voice"`).
    pub label: String,
    /// Whether in-order delivery is required.
    pub ordered: bool,
    /// Maximum packet lifetime in milliseconds.
    ///
    /// Maps to [`str0m::channel::Reliability::MaxPacketLifetime`].
    /// `None` = use retransmit policy or reliable delivery.
    pub max_packet_lifetime_ms: Option<u16>,
    /// Maximum number of retransmits before the packet is discarded.
    ///
    /// Maps to [`str0m::channel::Reliability::MaxRetransmits`].
    /// `None` = use lifetime or reliable delivery.
    pub max_retransmits: Option<u16>,
}

impl ChannelConfig {
    /// Reliable, ordered DataChannel (TCP-like semantics).
    ///
    /// Used by `with_chat_dcs` for the `"chat-data"` channel (id=4).
    #[must_use]
    pub fn reliable_ordered() -> Self {
        Self {
            id: 0,
            label: String::new(),
            ordered: true,
            max_packet_lifetime_ms: None,
            max_retransmits: None,
        }
    }

    /// Unordered, unreliable DataChannel with a max-retransmit count.
    ///
    /// Pass `retransmits = 0` for fire-and-forget semantics.
    /// Used by `with_chat_dcs` for the `"chat-ctrl"` channel (id=5).
    #[must_use]
    pub fn unreliable_max_retransmits(retransmits: u16) -> Self {
        Self {
            id: 0,
            label: String::new(),
            ordered: false,
            max_packet_lifetime_ms: None,
            max_retransmits: Some(retransmits),
        }
    }

    /// Unordered, unreliable DataChannel with a max-packet-lifetime in milliseconds.
    ///
    /// Maps to [`str0m::channel::Reliability::MaxPacketLifetime`].
    /// Used by `with_voice_dc` for low-latency voice data signalling (id=6).
    #[must_use]
    pub fn unreliable_max_lifetime(lifetime_ms: u16) -> Self {
        Self {
            id: 0,
            label: String::new(),
            ordered: false,
            max_packet_lifetime_ms: Some(lifetime_ms),
            max_retransmits: None,
        }
    }
}
