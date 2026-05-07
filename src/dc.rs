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
/// One `ChannelConfig` is pushed into the client's DC list by every
/// `with_extra_dc` / `with_chat_dcs` / `with_voice_dc` call.
/// The SFU application (e.g. `partner-edge`) reads these out via
/// [`Client::extra_dcs()`][crate::Client::extra_dcs] during
/// offer/answer negotiation and passes them to `Rtc::open_stream`.
///
/// # Invariant
///
/// Exactly one of `max_packet_lifetime_ms` and `max_retransmits` is `Some`
/// for unreliable channels; both are `None` for reliable channels.
/// Both being `Some` simultaneously is undefined behaviour in str0m.
/// The three constructors enforce this; constructing `ChannelConfig` via
/// struct literal outside this module is not possible (fields are
/// `pub(crate)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelConfig {
    /// SCTP stream identifier. Must be unique per peer-connection.
    pub(crate) id: u16,
    /// Human-readable channel label (e.g. `"chat-data"`, `"voice"`).
    pub(crate) label: String,
    /// Whether in-order delivery is required.
    pub(crate) ordered: bool,
    /// Maximum packet lifetime in milliseconds.
    ///
    /// Maps to [`str0m::channel::Reliability::MaxPacketLifetime`].
    /// `None` = use retransmit policy or reliable delivery.
    ///
    /// Stored as `u32` for range parity with the spec contract.
    /// Note: str0m's internal `Reliability::MaxPacketLifetime.lifetime`
    /// is `u16`; callers converting to str0m must `try_into::<u16>()`.
    pub(crate) max_packet_lifetime_ms: Option<u32>,
    /// Maximum number of retransmits before the packet is discarded.
    ///
    /// Maps to [`str0m::channel::Reliability::MaxRetransmits`].
    /// `None` = use lifetime or reliable delivery.
    pub(crate) max_retransmits: Option<u16>,
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
    ///
    /// Note: str0m's internal representation uses `u16`; callers converting to
    /// str0m must `try_into::<u16>()` on the stored value.
    #[must_use]
    pub fn unreliable_max_lifetime(lifetime_ms: u32) -> Self {
        Self {
            id: 0,
            label: String::new(),
            ordered: false,
            max_packet_lifetime_ms: Some(lifetime_ms),
            max_retransmits: None,
        }
    }

    /// SCTP stream identifier. Must be unique per peer-connection.
    #[must_use]
    pub fn id(&self) -> u16 {
        self.id
    }

    /// Human-readable channel label (e.g. `"chat-data"`, `"voice"`).
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Whether in-order delivery is required.
    #[must_use]
    pub fn ordered(&self) -> bool {
        self.ordered
    }

    /// Maximum packet lifetime in milliseconds, if set.
    ///
    /// `Some` only for unreliable channels using `Reliability::MaxPacketLifetime`.
    /// Mutually exclusive with [`max_retransmits`][Self::max_retransmits].
    #[must_use]
    pub fn max_packet_lifetime_ms(&self) -> Option<u32> {
        self.max_packet_lifetime_ms
    }

    /// Maximum number of retransmits before discarding, if set.
    ///
    /// `Some` only for unreliable channels using `Reliability::MaxRetransmits`.
    /// Mutually exclusive with [`max_packet_lifetime_ms`][Self::max_packet_lifetime_ms].
    #[must_use]
    pub fn max_retransmits(&self) -> Option<u16> {
        self.max_retransmits
    }
}
