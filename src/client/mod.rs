//! Per-peer state machine wrapping a str0m [`Rtc`] instance.
//!
//! Ported from [`str0m/examples/chat.rs`](https://github.com/algesten/str0m/blob/0.18.0/examples/chat.rs)
//! with multi-client fanout, simulcast layer filtering, and keyframe-request
//! plumbing added.
//!
//! Outbound UDP is parked on `pending_out`; the registry drains it between
//! polls (str0m is sync, the run-loop is tokio).
//!
//! Submodules: [`keyframe`], [`fanout`], [`layer`], [`tracks`].

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Weak};
use std::time::Instant;

use str0m::media::{KeyframeRequestKind, MediaData, MediaKind, Mid, Rid};
use str0m::{Event, IceConnectionState, Output, Rtc};

use crate::ids::SfuRid;
use crate::metrics::SfuMetrics;
use crate::net::{IncomingDatagram, OutgoingDatagram};
use crate::propagate::{ClientId, Propagated};

pub mod accessors;
pub mod construct;
pub mod fanout;
pub mod keyframe;
pub mod layer;
pub mod stats;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_seed;
pub mod tracks;

pub use tracks::TrackIn;
use tracks::{TrackInEntry, TrackOut, TrackOutState};

/// Per-peer state machine wrapping a str0m [`Rtc`] instance.
///
/// One `Client` exists per connected peer in the room. The registry owns all
/// clients and is the single entity that drives them via [`poll_output`][Client::poll_output]
/// and [`handle_input`][Client::handle_input].
#[derive(Debug)]
pub struct Client {
    /// Process-unique identifier for this peer.
    pub id: ClientId,
    /// Whether this client is a local peer or an upstream SFU relay.
    pub(crate) origin: crate::origin::ClientOrigin,
    pub(crate) rtc: Rtc,
    pub(crate) tracks_in: Vec<TrackInEntry>,
    pub(crate) tracks_out: Vec<TrackOut>,
    /// Last simulcast RID actually forwarded to this peer. `None` = no simulcast yet.
    pub(crate) chosen_rid: Option<Rid>,
    /// Preferred simulcast layer (default [`layer::LOW`]).
    pub(crate) desired_layer: SfuRid,
    /// Simulcast RIDs this peer has been observed publishing.
    /// Populated on every incoming `MediaData`. Empty = bootstrap / non-simulcast.
    pub(crate) active_rids: HashSet<SfuRid>,
    /// Outbound datagrams pending flush by the registry.
    pub(crate) pending_out: VecDeque<str0m::net::Transmit>,
    /// Prometheus handles (shared with the registry when inserted).
    pub(crate) metrics: Arc<SfuMetrics>,
    /// Post-layer-filter forwarded-media counter (readable by integration tests).
    pub(crate) delivered_media: AtomicU64,
    /// `ActiveSpeakerChanged` deliveries (skip-self check in tests).
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) delivered_active_speaker: AtomicU64,
    /// Per-subscriber hysteretic layer pacer driven from egress BWE readings.
    #[cfg(feature = "pacer")]
    pub(crate) pacer: crate::bwe::SubscriberPacer,
    /// Maximum AV1 temporal layer to forward to this subscriber (default = all).
    #[cfg(feature = "av1-dd")]
    pub(crate) max_temporal_layer: u8,
    /// Maximum RFC 9626 temporal layer to forward to this subscriber (default = all).
    #[cfg(feature = "vfm")]
    pub(crate) max_vfm_temporal_layer: u8,
}

impl Client {
    /// Feed a demuxed UDP datagram into str0m.
    pub fn handle_input(&mut self, datagram: IncomingDatagram) {
        if !self.rtc.is_alive() {
            return;
        }
        let contents = match (&datagram.contents[..]).try_into() {
            Ok(c) => c,
            Err(_) => {
                tracing::debug!(client = *self.id, "ignoring empty or invalid datagram");
                return;
            }
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
        if let Err(e) = self.rtc.handle_input(input) {
            tracing::warn!(client = *self.id, error = ?e, "client disconnected on handle_input");
            self.rtc.disconnect();
        }
    }

    /// Feed a timeout event into str0m (internal use by registry tick).
    pub(crate) fn handle_timeout(&mut self, at: Instant) {
        if !self.rtc.is_alive() {
            return;
        }
        if let Err(e) = self.rtc.handle_input(str0m::Input::Timeout(at)) {
            tracing::warn!(client = *self.id, error = ?e, "client disconnected on timeout");
            self.rtc.disconnect();
        }
    }

    /// Drive str0m forward one step.
    ///
    /// Outbound UDP datagrams are appended to `pending_out`; the registry drains
    /// them between polls via [`drain_pending_out`][Client::drain_pending_out].
    pub fn poll_output(&mut self) -> Propagated {
        if !self.rtc.is_alive() {
            return Propagated::Noop;
        }
        match self.rtc.poll_output() {
            Ok(output) => self.handle_output(output),
            Err(e) => {
                tracing::warn!(client = *self.id, error = ?e, "poll_output failed");
                self.rtc.disconnect();
                Propagated::Noop
            }
        }
    }

    fn handle_output(&mut self, output: Output) -> Propagated {
        match output {
            Output::Transmit(t) => {
                self.pending_out.push_back(t);
                Propagated::Noop
            }
            Output::Timeout(t) => Propagated::Timeout(t),
            Output::Event(e) => self.handle_event(e),
        }
    }

    fn handle_event(&mut self, event: Event) -> Propagated {
        match event {
            Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                self.rtc.disconnect();
                Propagated::Noop
            }
            Event::MediaAdded(m) => self.track_in_added(m.mid, m.kind),
            Event::MediaData(data) => self.track_in_media(data),
            Event::KeyframeRequest(req) => self.incoming_keyframe_req(req),
            Event::EgressBitrateEstimate(bwe) => stats::propagate_bwe(self.id, bwe),
            Event::PeerStats(s) => stats::propagate_peer_stats(self.id, s),
            _ => Propagated::Noop,
        }
    }

    fn track_in_added(&mut self, mid: Mid, kind: MediaKind) -> Propagated {
        let entry = TrackInEntry {
            id: Arc::new(TrackIn {
                origin: self.id,
                mid,
                kind,
                relay_source: self.is_relay(),
            }),
            last_keyframe_request: None,
        };
        let weak = Arc::downgrade(&entry.id);
        self.tracks_in.push(entry);
        Propagated::TrackOpen(self.id, weak)
    }

    fn track_in_media(&mut self, data: MediaData) -> Propagated {
        if !data.contiguous {
            self.request_keyframe_throttled(data.mid, data.rid, KeyframeRequestKind::Fir);
        }
        if let Some(rid) = data.rid {
            self.active_rids.insert(SfuRid::from_str0m(rid));
        }
        Propagated::MediaData(self.id, crate::media::SfuMediaPayload::from_str0m(data))
    }

    /// Register that another client opened a track we should mirror to this peer.
    pub fn handle_track_open(&mut self, track_in: Weak<TrackIn>) {
        self.tracks_out.push(TrackOut {
            track_in,
            state: TrackOutState::ToOpen,
        });
    }

    /// Drain queued outbound datagrams.
    ///
    /// The registry calls this after each poll cycle to pass bytes to the tokio socket.
    pub fn drain_pending_out(&mut self) -> impl Iterator<Item = OutgoingDatagram> + '_ {
        std::mem::take(&mut self.pending_out)
            .into_iter()
            .map(OutgoingDatagram::from_transmit)
    }
}
