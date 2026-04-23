//! Cross-client propagated events.
//!
//! Only events that fan out between clients live here. Outbound UDP
//! `Transmit`s are held on the [`Client`][crate::Client] and drained by the
//! registry — they never propagate.
//!
//! Ported from [`str0m/examples/chat.rs`](https://github.com/algesten/str0m/blob/0.18.0/examples/chat.rs).

use std::ops::Deref;
use std::sync::Weak;
use std::time::Instant;

use crate::bandwidth::BandwidthEstimate;
use crate::client::TrackIn;
use crate::ids::{SfuMid, SfuRid};
use crate::keyframe::SfuKeyframeRequest;
use crate::media::SfuMediaPayload;
use crate::rtcp_stats::PeerRtcpStats;

/// Monotonic per-process identifier for a connected peer.
///
/// Wraps a `u64` counter allocated at `Client` construction time. Implements
/// [`Deref`] to `u64` for ergonomic comparisons with the speaker-detection
/// API that uses bare `u64` peer IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientId(pub u64);

impl Deref for ClientId {
    type Target = u64;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Events the registry propagates between clients.
///
/// `Noop` and `Timeout` are consumed inside the registry's poll loop and never
/// reach individual clients. All other variants fan out to every non-origin peer.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Propagated {
    /// Nothing to do — returned by [`Client::poll_output`][crate::Client::poll_output]
    /// when str0m produced only outbound datagrams (queued on the client).
    Noop,

    /// The client's poll returned this as its next wake-up deadline.
    Timeout(Instant),

    /// A new incoming track is open on the originating client and should be
    /// advertised to every other client.
    TrackOpen(ClientId, Weak<TrackIn>),

    /// Media payload received by the originating client, to be forwarded to
    /// every other client (subject to the per-subscriber simulcast layer filter).
    MediaData(ClientId, SfuMediaPayload),

    /// A keyframe request that must reach the source of the outgoing track.
    ///
    /// Fields: `(origin_of_request, request, source_client, source_mid)`.
    /// The fanout dispatcher routes this only to the `source_client`.
    KeyframeRequest(ClientId, SfuKeyframeRequest, ClientId, SfuMid),

    /// A keyframe request that must be forwarded upstream to the origin SFU.
    ///
    /// Emitted instead of [`KeyframeRequest`][Self::KeyframeRequest] when a
    /// subscriber requests a keyframe for a track whose publisher is a relay
    /// client (`ClientOrigin::RelayFromSfu`). The application must relay this
    /// request to the upstream SFU via its signalling channel -- the kit cannot
    /// send PLI/FIR to a relay peer that has no inbound WebRTC negotiation for
    /// that direction.
    ///
    /// Fields: `(source_relay_id, req, source_mid)`.
    UpstreamKeyframeRequest {
        /// The relay client whose upstream track needs a keyframe.
        source_relay_id: ClientId,
        /// The keyframe request (PLI or FIR).
        req: SfuKeyframeRequest,
        /// The track MID on the relay client.
        source_mid: SfuMid,
    },

    /// Dominant-speaker election changed.
    ///
    /// Emitted by [`Registry::tick_active_speaker`][crate::Registry::tick_active_speaker]
    /// when the `active-speaker` feature is enabled. The `peer_id` is the newly
    /// dominant peer. Fanout skips the speaker themselves (skip-self rule).
    #[cfg(feature = "active-speaker")]
    #[cfg_attr(docsrs, doc(cfg(feature = "active-speaker")))]
    ActiveSpeakerChanged {
        /// The peer that became the dominant speaker.
        peer_id: u64,
        /// Medium-window log-ratio confidence margin (C2).
        ///
        /// `0.0` means bootstrap election (first speaker in an empty room).
        /// Values above `2.0` indicate a confident, contested win.
        /// Consumers may use this to delay UI updates for low-confidence switches.
        confidence: f64,
    },

    /// Egress bandwidth estimate updated for this peer.
    ///
    /// Emitted from str0m's internal GoogCC each time the estimator produces a new
    /// value (typically every 100–500 ms depending on TWCC traffic). Downstream
    /// should consume this to drive layer selection or pacing decisions.
    BandwidthEstimate {
        /// The peer whose egress estimate changed.
        peer_id: ClientId,
        /// The new estimate.
        estimate: BandwidthEstimate,
    },

    /// RTCP-derived stats updated for this peer.
    ///
    /// Derived from str0m's `Event::PeerStats` (emitted ~1 Hz). Contains
    /// loss fraction and RTT; jitter is not available from the per-peer aggregate
    /// event in str0m 0.18 (it requires per-mid `MediaEgressStats`) and is
    /// always `Duration::ZERO` in this release.
    RtcpStats {
        /// The peer whose stats were updated.
        peer_id: ClientId,
        /// The updated stats snapshot.
        stats: PeerRtcpStats,
    },

    /// Subscriber's egress BWE crossed the audio-only threshold.
    ///
    /// When `audio_only = true`, stop forwarding video to this peer.
    /// When `audio_only = false`, resume. Only emitted with `pacer` feature.
    #[cfg(feature = "pacer")]
    #[cfg_attr(docsrs, doc(cfg(feature = "pacer")))]
    AudioOnlyMode {
        /// The subscriber peer.
        peer_id: ClientId,
        /// `true` = entered audio-only; `false` = video restored.
        audio_only: bool,
    },
    /// Hint to the publisher that they may stop encoding layers above `max_rid`.
    ///
    /// Emitted by [`Registry::emit_publisher_layer_hints`] when the maximum
    /// desired layer across all subscribers changes. The application should relay
    /// this to the publisher via RTCP or signalling.
    PublisherLayerHint {
        /// The publisher whose encoding may be reduced.
        publisher_id: ClientId,
        /// Highest simulcast layer any subscriber currently wants.
        max_rid: SfuRid,
    },

    /// Hint to the application that the upstream SFU should stop encoding layers
    /// above  for this relay publisher.
    ///
    /// Emitted by [] when the maximum
    /// desired layer across all subscribers of a relay-originated publisher changes.
    /// The application must forward this via its inter-SFU signalling channel.
    PublisherLayerHintForUpstream {
        /// The relay client whose upstream publisher should be signalled.
        publisher_relay_id: ClientId,
        /// Highest simulcast layer any subscriber of this relay currently wants.
        max_rid: SfuRid,
    },

    /// Subscriber capability hint for Opus audio codec redundancy.
    ///
    /// Emit to the application signalling layer to negotiate `red/48000/2` in
    /// the publisher's SDP offer, or relay via a custom data-channel protocol.
    /// The SFU does not inject RED — it is a sender-side concern.
    AudioCodecHint {
        /// The subscriber expressing the preference.
        peer_id: ClientId,
        /// Subscriber can decode Opus RFC 2198 RED (`red/48000/2` in SDP).
        opus_red: bool,
        /// Subscriber can decode Opus DRED (Deep REDundancy — libopus 0.9+).
        opus_dred: bool,
    },
}

impl Propagated {
    /// Which client produced this event, if any.
    ///
    /// Used by the registry to skip the originator during fanout. Returns `None`
    /// for `Noop`, `Timeout`, and `ActiveSpeakerChanged` (the latter uses its
    /// own `peer_id == *client.id` skip rule).
    pub fn client_id(&self) -> Option<ClientId> {
        match self {
            Propagated::TrackOpen(c, _)
            | Propagated::MediaData(c, _)
            | Propagated::KeyframeRequest(c, _, _, _) => Some(*c),
            Propagated::Noop | Propagated::Timeout(_) => None,
            #[cfg(feature = "active-speaker")]
            Propagated::ActiveSpeakerChanged { .. } => None,
            Propagated::BandwidthEstimate { peer_id, .. }
            | Propagated::RtcpStats { peer_id, .. } => Some(*peer_id),
            #[cfg(feature = "pacer")]
            Propagated::AudioOnlyMode { peer_id, .. } => Some(*peer_id),
            Propagated::PublisherLayerHint { publisher_id, .. } => Some(*publisher_id),
            Propagated::PublisherLayerHintForUpstream {
                publisher_relay_id, ..
            } => Some(*publisher_relay_id),
            Propagated::AudioCodecHint { peer_id, .. } => Some(*peer_id),
            Propagated::UpstreamKeyframeRequest {
                source_relay_id, ..
            } => Some(*source_relay_id),
        }
    }
}
