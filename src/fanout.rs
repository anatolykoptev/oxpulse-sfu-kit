//! Cross-client event fanout.
//!
//! Separate from [`registry`][crate::registry] — that module owns routing UDP
//! to the correct client and polling; this module owns the "deliver one
//! [`Propagated`] event to every non-origin client" logic.
//!
//! The simulcast filter lives deeper, in
//! [`client::fanout::handle_media_data_out`][crate::client::Client::handle_media_data_out],
//! so this module just dispatches the right method per variant.

use crate::client::Client;
use crate::propagate::Propagated;

/// Apply a single propagated event to every client except the originator.
///
/// `pub(crate)` so the registry and test seams can call it without exposing it
/// on the public API surface.
pub(crate) fn fanout(p: &Propagated, clients: &mut [Client]) {
    #[cfg(feature = "active-speaker")]
    if let Propagated::ActiveSpeakerChanged { peer_id, .. } = p {
        for client in clients.iter_mut() {
            if *client.id == *peer_id {
                // Skip-self: the speaker doesn't receive their own dominance event.
                continue;
            }
            client.handle_active_speaker_changed(*peer_id);
        }
        return;
    }

    let Some(origin) = p.client_id() else {
        return;
    };
    for client in clients.iter_mut() {
        if client.id == origin {
            continue;
        }
        match p {
            Propagated::TrackOpen(_, track_in) => client.handle_track_open(track_in.clone()),
            Propagated::MediaData(_, data) => client.handle_media_data_out(origin, data),
            Propagated::KeyframeRequest(_, req, source, mid_in) => {
                if *source == client.id {
                    client.handle_keyframe_request(*req, *mid_in);
                }
            }
            Propagated::Noop
            | Propagated::Timeout(_)
            | Propagated::BandwidthEstimate { .. }
            | Propagated::RtcpStats { .. }
            | Propagated::PublisherLayerHint { .. }
            | Propagated::PublisherLayerHintForUpstream { .. }
            | Propagated::AudioCodecHint { .. }
            | Propagated::UpstreamKeyframeRequest { .. } => {}
            #[cfg(feature = "active-speaker")]
            Propagated::ActiveSpeakerChanged { .. } => {}
            #[cfg(feature = "pacer")]
            Propagated::AudioOnlyMode { .. } => {}
        }
    }
}

/// Drive `fanout` against a caller-owned `&mut [Client]`.
///
/// Exposed for integration tests that want to exercise fanout semantics without
/// running the full async UDP loop.
#[cfg(any(test, feature = "test-utils"))]
#[doc(hidden)]
pub fn fanout_for_tests(p: &Propagated, clients: &mut [Client]) {
    fanout(p, clients);
}
