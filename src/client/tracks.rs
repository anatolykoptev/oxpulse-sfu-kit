//! Track-side data types.
//!
//! Describes what a peer publishes (`TrackIn*`) or receives from another peer
//! (`TrackOut*`). Kept separate from `client/mod.rs` so the state machine
//! can focus on `Rtc`-driven event dispatch.

use std::sync::{Arc, Weak};
use std::time::Instant;

use str0m::media::{MediaKind, Mid};

use crate::propagate::ClientId;

/// An incoming track advertised by a client.
///
/// The originating client owns the strong `Arc`; every other client that
/// subscribes holds a `Weak`. When the publisher disconnects the `Arc` drops
/// and all subscriber `Weak`s become invalid.
#[derive(Debug)]
pub struct TrackIn {
    /// The peer that is publishing this track.
    pub origin: ClientId,
    /// str0m media identifier.
    pub mid: Mid,
    /// Audio or video.
    pub kind: MediaKind,
    /// `true` if the publishing client is a relay from another SFU edge.
    ///
    /// Set at track-open time from the publisher's `is_relay()` status.
    /// Used by the subscriber's keyframe-request path to emit
    /// [`Propagated::UpstreamKeyframeRequest`][crate::Propagated::UpstreamKeyframeRequest]
    /// instead of a direct PLI/FIR.
    pub relay_source: bool,
}

#[derive(Debug)]
pub(crate) struct TrackInEntry {
    pub id: Arc<TrackIn>,
    pub last_keyframe_request: Option<Instant>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrackOutState {
    ToOpen,
    Negotiating(Mid),
    Open(Mid),
}

#[derive(Debug)]
pub(crate) struct TrackOut {
    pub track_in: Weak<TrackIn>,
    pub state: TrackOutState,
}

impl TrackOut {
    pub(crate) fn mid(&self) -> Option<Mid> {
        match self.state {
            TrackOutState::ToOpen => None,
            TrackOutState::Negotiating(m) | TrackOutState::Open(m) => Some(m),
        }
    }
}
