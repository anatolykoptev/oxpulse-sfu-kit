//! `Client` construction — wraps a fresh `Rtc`, allocates a process-unique
//! `ClientId`, and initialises every field to its zero-state default.

use std::collections::{HashSet, VecDeque};
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
        Self {
            id: next_client_id(),
            rtc: rtc.0,
            tracks_in: Vec::new(),
            tracks_out: Vec::new(),
            chosen_rid: None,
            desired_layer: layer::LOW,
            active_rids: HashSet::new(),
            pending_out: VecDeque::new(),
            metrics,
            delivered_media: AtomicU64::new(0),
            #[cfg(any(test, feature = "test-utils"))]
            delivered_active_speaker: AtomicU64::new(0),
            #[cfg(feature = "pacer")]
            pacer: crate::bwe::SubscriberPacer::new(),
            #[cfg(feature = "av1-dd")]
            max_temporal_layer: u8::MAX, // default: forward all temporal layers
        }
    }
}
