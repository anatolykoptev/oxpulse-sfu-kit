//! Public read/write accessors on [`Client`][super::Client].
//!
//! Separated from the core poll/event loop in `mod.rs` to keep that file
//! focused on str0m I/O driving.

use std::sync::atomic::Ordering;

use str0m::media::Rid;
use str0m::Input;

use super::Client;

impl Client {
    /// This subscriber's current desired simulcast layer.
    pub fn desired_layer(&self) -> Rid {
        self.desired_layer
    }

    /// Override this subscriber's desired simulcast layer.
    ///
    /// Takes effect on the next forwarded packet; no SDP renegotiation required.
    pub fn set_desired_layer(&mut self, rid: Rid) {
        self.desired_layer = rid;
        // Invalidate the cached chosen layer so keyframe requests target the
        // correct RID on the next forwarded packet.
        self.chosen_rid = None;
    }

    /// Simulcast RIDs the peer has been observed publishing.
    ///
    /// Built up incrementally on each received `MediaData`. Empty until the
    /// first video packet arrives. Callers that use this as the "available
    /// layers" input should fall back to the full ladder (`[LOW, MEDIUM, HIGH]`)
    /// when empty — before the first packet the full ladder is the correct assumption.
    pub fn active_rids(&self) -> Vec<Rid> {
        self.active_rids.iter().copied().collect()
    }

    /// Number of `MediaData` events forwarded to this client after layer filtering.
    pub fn delivered_media_count(&self) -> u64 {
        self.delivered_media.load(Ordering::Relaxed)
    }

    /// Number of `ActiveSpeakerChanged` events delivered to this client.
    ///
    /// Only available with `test-utils` feature; used to verify skip-self semantics.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn delivered_active_speaker_count(&self) -> u64 {
        self.delivered_active_speaker.load(Ordering::Relaxed)
    }

    /// Whether the underlying str0m `Rtc` is still alive.
    pub fn is_alive(&self) -> bool {
        self.rtc.is_alive()
    }

    /// str0m demux probe — returns `true` if this client owns the given datagram.
    ///
    /// Used by the registry to route incoming UDP to the correct peer.
    pub fn accepts(&self, input: &Input) -> bool {
        self.rtc.accepts(input)
    }
}
