//! `Client` construction — wraps a fresh `Rtc`, allocates a process-unique
//! `ClientId`, and initialises every field to its zero-state default.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::{layer, Client};
use crate::metrics::SfuMetrics;
use crate::propagate::ClientId;
use crate::rtc::SfuRtc;

fn next_client_id() -> ClientId {
    static ID_COUNTER: AtomicU64 = AtomicU64::new(0);
    ClientId(ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}

impl Client {
    /// Wrap a freshly-created [`SfuRtc`] instance.
    ///
    /// The `metrics` handle is replaced by the registry's own instance when
    /// [`Registry::insert`][crate::Registry::insert] is called, so all counters
    /// from all clients flow to the same Prometheus registry.
    pub fn new(rtc: SfuRtc, metrics: Arc<SfuMetrics>) -> Self {
        let id = next_client_id();
        // F7-1: pre-resolve the per-peer drop counter so the fanout hot path
        // is a single atomic add with no per-frame `to_string()` alloc.
        #[cfg(feature = "metrics-prometheus")]
        let video_frames_dropped = metrics.peer_drop_counter(*id);
        Self {
            id,
            origin: crate::origin::ClientOrigin::Local,
            #[cfg(feature = "active-speaker")]
            in_speaker_detector: false,
            rtc: rtc.0,
            tracks_in: Vec::new(),
            tracks_out: Vec::new(),
            chosen_rid: None,
            desired_layer: layer::LOW,
            active_rids: HashSet::new(),
            mid_to_kind: HashMap::new(),
            pending_out: VecDeque::new(),
            metrics,
            delivered_media: AtomicU64::new(0),
            #[cfg(feature = "metrics-prometheus")]
            video_frames_dropped,
            #[cfg(any(test, feature = "test-utils"))]
            delivered_active_speaker: AtomicU64::new(0),
            #[cfg(feature = "pacer")]
            pacer: crate::bwe::SubscriberPacer::new(),
            #[cfg(feature = "pacer")]
            suspended: false,
            #[cfg(all(feature = "pacer", feature = "kalman-bwe"))]
            last_pacer_drive: None,
            #[cfg(feature = "av1-dd")]
            max_temporal_layer: u8::MAX, // default: forward all temporal layers
            #[cfg(feature = "vfm")]
            max_vfm_temporal_layer: u8::MAX,
            extra_dcs: Vec::new(),
        }
    }
}
