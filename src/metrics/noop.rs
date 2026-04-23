//! No-op [`SfuMetrics`] stub used when `metrics-prometheus` feature is off.
//!
//! All methods compile away to nothing. `Client` and `Registry` always hold
//! an `Arc<SfuMetrics>` without any `#[cfg]` at call sites.

/// No-op metrics stub — all methods are zero-cost no-ops.
#[derive(Clone, Debug, Default)]
pub struct SfuMetrics;

impl SfuMetrics {
    /// No-op constructor — metrics are disabled. Always succeeds.
    pub fn new_default() -> Self {
        Self
    }

    pub(crate) fn inc_forwarded_packets(&self, _kind: &str) {}
    pub(crate) fn inc_layer_selection(&self, _layer: &str) {}
    pub(crate) fn inc_client_connect(&self) {}
    pub(crate) fn inc_client_disconnect(&self) {}
    pub(crate) fn inc_active_participants(&self) {}
    pub(crate) fn dec_active_participants(&self) {}
    #[cfg(feature = "active-speaker")]
    pub(crate) fn inc_dominant_speaker_changes(&self) {}
    pub(crate) fn update_peer_rtcp(
        &self,
        _peer_id: u64,
        _loss: f32,
        _rtt_ms: f64,
        _jitter_ms: f64,
    ) {
    }
    pub(crate) fn update_peer_bwe(&self, _peer_id: u64, _bps: u64) {}
    #[cfg(feature = "active-speaker")]
    pub(crate) fn update_peer_speaker_scores(
        &self,
        _peer_id: u64,
        _immediate: f64,
        _medium: f64,
        _long_score: f64,
    ) {
    }
    /// Noop cardinality reaper — no-op when Prometheus is disabled.
    pub fn reap_dead_peer(&self, _peer_id: u64) {}
}
