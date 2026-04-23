//! Public read/write accessors on [`Client`].
//!
//! Separated from the core poll/event loop in `mod.rs` to keep that file
//! focused on str0m I/O driving.

use std::sync::atomic::Ordering;

use super::Client;
use crate::ids::SfuRid;
use crate::net::IncomingDatagram;

impl Client {
    /// This subscriber's current desired simulcast layer.
    #[must_use]
    pub fn desired_layer(&self) -> SfuRid {
        self.desired_layer
    }

    /// Override this subscriber's desired simulcast layer.
    ///
    /// Takes effect on the next forwarded packet; no SDP renegotiation required.
    pub fn set_desired_layer(&mut self, rid: SfuRid) {
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
    #[must_use]
    pub fn active_rids(&self) -> Vec<SfuRid> {
        self.active_rids.iter().copied().collect()
    }

    /// Number of `MediaData` events forwarded to this client after layer filtering.
    #[must_use]
    pub fn delivered_media_count(&self) -> u64 {
        self.delivered_media.load(Ordering::Relaxed)
    }

    /// Number of `ActiveSpeakerChanged` events delivered to this client.
    ///
    /// Only available with `test-utils` feature; used to verify skip-self semantics.
    #[cfg(any(test, feature = "test-utils"))]
    #[must_use]
    pub fn delivered_active_speaker_count(&self) -> u64 {
        self.delivered_active_speaker.load(Ordering::Relaxed)
    }

    /// Whether the underlying str0m `Rtc` is still alive.
    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.rtc.is_alive()
    }

    /// Demux probe — returns `true` if this client owns the given datagram.
    ///
    /// Used by the registry to route incoming UDP to the correct peer.
    #[must_use]
    pub fn accepts(&self, datagram: &IncomingDatagram) -> bool {
        let Ok(contents) = (&datagram.contents[..]).try_into() else {
            return false;
        };
        let input = str0m::Input::Receive(
            datagram.received_at,
            str0m::net::Receive {
                proto: datagram.proto.to_str0m(),
                source: datagram.source,
                destination: datagram.destination,
                contents,
            },
        );
        self.rtc.accepts(&input)
    }

    /// Feed a new egress BWE reading to this subscriber's pacer.
    ///
    /// If the action is `PacerAction::ChangeLayer`, `desired_layer` is updated
    /// in-place before returning. For `GoAudioOnly` / `RestoreVideo`, the registry
    /// should emit `Propagated::AudioOnlyMode`.
    ///
    /// Only available with the `pacer` feature.
    #[cfg(feature = "pacer")]
    #[cfg_attr(docsrs, doc(cfg(feature = "pacer")))]
    pub fn drive_pacer(&mut self, bps: u64) -> crate::bwe::PacerAction {
        let action = self.pacer.update(bps);
        if let crate::bwe::PacerAction::ChangeLayer(rid) = action {
            self.set_desired_layer(rid);
        }
        action
    }

    /// Set the maximum AV1 temporal layer to forward to this subscriber.
    ///
    /// Packets with `temporal_id > max` are dropped at fanout.
    /// Default is `u8::MAX` (all layers forwarded).
    ///
    /// Only available with the `av1-dd` feature.
    #[cfg(feature = "av1-dd")]
    #[cfg_attr(docsrs, doc(cfg(feature = "av1-dd")))]
    pub fn set_max_temporal_layer(&mut self, max: u8) {
        self.max_temporal_layer = max;
    }

    /// Current AV1 temporal layer cap.
    ///
    /// Only available with the `av1-dd` feature.
    #[cfg(feature = "av1-dd")]
    #[cfg_attr(docsrs, doc(cfg(feature = "av1-dd")))]
    #[must_use]
    pub fn max_temporal_layer(&self) -> u8 {
        self.max_temporal_layer
    }

    /// Set the maximum RFC 9626 temporal layer to forward to this subscriber.
    ///
    /// Packets with `temporal_id > max` are dropped at fanout.
    /// Default: `u8::MAX` (all layers forwarded).
    #[cfg(feature = "vfm")]
    #[cfg_attr(docsrs, doc(cfg(feature = "vfm")))]
    pub fn set_max_vfm_temporal_layer(&mut self, max: u8) {
        self.max_vfm_temporal_layer = max;
    }

    /// Current RFC 9626 temporal layer cap.
    #[cfg(feature = "vfm")]
    #[cfg_attr(docsrs, doc(cfg(feature = "vfm")))]
    #[must_use]
    pub fn max_vfm_temporal_layer(&self) -> u8 {
        self.max_vfm_temporal_layer
    }

    /// This client's origin (local peer or upstream SFU relay).
    #[must_use]
    pub fn origin(&self) -> &crate::origin::ClientOrigin {
        &self.origin
    }

    /// Override the client origin.
    ///
    /// Must be called **before** [`Registry::insert`][crate::Registry::insert].
    /// See [`ClientOrigin`][crate::origin::ClientOrigin] for the call-order contract.
    pub fn set_origin(&mut self, origin: crate::origin::ClientOrigin) {
        self.origin = origin;
    }

    /// Returns `true` if this client is a relay connection from another SFU edge.
    #[must_use]
    pub fn is_relay(&self) -> bool {
        matches!(self.origin, crate::origin::ClientOrigin::RelayFromSfu(_))
    }
}
