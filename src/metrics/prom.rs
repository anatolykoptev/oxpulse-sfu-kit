//! Prometheus-backed [`SfuMetrics`] implementation.
//!
//! Active when the `metrics-prometheus` feature is enabled.

use std::sync::Arc;

use anyhow::Context;
use prometheus::{GaugeVec, IntCounter, IntCounterVec, IntGauge, Opts, Registry};

/// All Prometheus handles for the SFU.
///
/// Constructed via [`SfuMetrics::new`] (fallible) or [`SfuMetrics::default`]
/// (panics on error, suitable for tests and `main`).
///
/// The registry uses the `"sfu"` prefix, so metric names become `sfu_<name>`.
#[derive(Clone, Debug)]
pub struct SfuMetrics {
    /// The underlying Prometheus registry. Pass to your HTTP handler to expose `/metrics`.
    pub registry: Arc<Registry>,
    /// Current number of live clients in the room.
    pub active_participants: IntGauge,
    /// Forwarded RTP packets, labelled by `kind` = `audio` | `video` | `other`.
    pub forwarded_packets_total: IntCounterVec,
    /// Simulcast layer forwarding events per tier, `layer` = `q` | `h` | `f` | `other`.
    pub layer_selection_total: IntCounterVec,
    /// Times the dominant speaker changed (requires `active-speaker` feature).
    pub dominant_speaker_changes_total: IntCounter,
    /// Total clients connected.
    pub client_connect_total: IntCounter,
    /// Total clients disconnected.
    pub client_disconnect_total: IntCounter,
    /// Per-peer egress packet loss fraction (from RTCP RR), label: `peer_id`.
    pub peer_loss_fraction: GaugeVec,
    /// Per-peer jitter in milliseconds (always 0.0 in v0.2), label: `peer_id`.
    pub peer_jitter_ms: GaugeVec,
    /// Per-peer round-trip time in milliseconds (from RTCP RR), label: `peer_id`.
    pub peer_rtt_ms: GaugeVec,
    /// Per-peer egress bandwidth estimate in bits/s (from BWE), label: `peer_id`.
    pub bandwidth_estimate_bps: GaugeVec,
    /// Per-peer immediate-window speaker activity score, label: `peer_id`.
    pub speaker_immediate: GaugeVec,
    /// Per-peer medium-window speaker activity score, label: `peer_id`.
    pub speaker_medium: GaugeVec,
    /// Per-peer long-window speaker activity score, label: `peer_id`.
    pub speaker_long: GaugeVec,
}

impl SfuMetrics {
    /// Construct a metrics instance, registering all handles into a new Prometheus registry.
    pub fn new() -> anyhow::Result<Self> {
        let registry = Registry::new_custom(Some("sfu".into()), None).context("create registry")?;

        macro_rules! reg {
            ($m:expr) => {{
                let m = $m;
                registry
                    .register(Box::new(m.clone()))
                    .context("metric registration")?;
                m
            }};
        }

        let active_participants = reg!(IntGauge::with_opts(Opts::new(
            "active_participants",
            "Live client count",
        ))
        .context("active_participants")?);

        let forwarded_packets_total = reg!(IntCounterVec::new(
            Opts::new(
                "forwarded_packets_total",
                "Forwarded RTP packets by media kind"
            ),
            &["kind"],
        )
        .context("forwarded_packets_total")?);

        let layer_selection_total = reg!(IntCounterVec::new(
            Opts::new(
                "layer_selection_total",
                "Simulcast layer forwarding events by RID"
            ),
            &["layer"],
        )
        .context("layer_selection_total")?);

        let dominant_speaker_changes_total = reg!(IntCounter::with_opts(Opts::new(
            "dominant_speaker_changes_total",
            "Times dominant speaker changed",
        ))
        .context("dominant_speaker_changes_total")?);

        let client_connect_total = reg!(IntCounter::with_opts(Opts::new(
            "client_connect_total",
            "Total clients connected",
        ))
        .context("client_connect_total")?);

        let client_disconnect_total = reg!(IntCounter::with_opts(Opts::new(
            "client_disconnect_total",
            "Total clients disconnected",
        ))
        .context("client_disconnect_total")?);

        let peer_loss_fraction = reg!(GaugeVec::new(
            Opts::new("peer_loss_fraction", "Per-peer egress packet loss fraction"),
            &["peer_id"],
        )
        .context("peer_loss_fraction")?);

        let peer_jitter_ms = reg!(GaugeVec::new(
            Opts::new("peer_jitter_ms", "Per-peer jitter in milliseconds"),
            &["peer_id"],
        )
        .context("peer_jitter_ms")?);

        let peer_rtt_ms = reg!(GaugeVec::new(
            Opts::new("peer_rtt_ms", "Per-peer round-trip time in milliseconds"),
            &["peer_id"],
        )
        .context("peer_rtt_ms")?);

        let bandwidth_estimate_bps = reg!(GaugeVec::new(
            Opts::new(
                "bandwidth_estimate_bps",
                "Per-peer egress bandwidth estimate in bits/s"
            ),
            &["peer_id"],
        )
        .context("bandwidth_estimate_bps")?);

        let speaker_immediate = reg!(GaugeVec::new(
            Opts::new(
                "speaker_immediate_score",
                "Per-peer immediate-window speaker activity score"
            ),
            &["peer_id"],
        )
        .context("speaker_immediate_score")?);

        let speaker_medium = reg!(GaugeVec::new(
            Opts::new(
                "speaker_medium_score",
                "Per-peer medium-window speaker activity score"
            ),
            &["peer_id"],
        )
        .context("speaker_medium_score")?);

        let speaker_long = reg!(GaugeVec::new(
            Opts::new(
                "speaker_long_score",
                "Per-peer long-window speaker activity score"
            ),
            &["peer_id"],
        )
        .context("speaker_long_score")?);

        Ok(Self {
            registry: Arc::new(registry),
            active_participants,
            forwarded_packets_total,
            layer_selection_total,
            dominant_speaker_changes_total,
            client_connect_total,
            client_disconnect_total,
            peer_loss_fraction,
            peer_jitter_ms,
            peer_rtt_ms,
            bandwidth_estimate_bps,
            speaker_immediate,
            speaker_medium,
            speaker_long,
        })
    }

    /// Encode the registry in Prometheus text format 0.0.4.
    pub fn encode_text(&self) -> anyhow::Result<String> {
        use prometheus::{Encoder, TextEncoder};
        let mut buf = Vec::new();
        TextEncoder::new()
            .encode(&self.registry.gather(), &mut buf)
            .context("encode metrics")?;
        String::from_utf8(buf).context("utf8")
    }

    /// Infallible constructor for use at startup / in test helpers.
    pub fn new_default() -> Self {
        Self::new().expect("SfuMetrics::new at startup")
    }

    pub(crate) fn inc_forwarded_packets(&self, kind: &str) {
        self.forwarded_packets_total
            .with_label_values(&[kind])
            .inc();
    }

    pub(crate) fn inc_layer_selection(&self, layer: &str) {
        self.layer_selection_total.with_label_values(&[layer]).inc();
    }

    pub(crate) fn inc_client_connect(&self) {
        self.client_connect_total.inc();
    }
    pub(crate) fn inc_client_disconnect(&self) {
        self.client_disconnect_total.inc();
    }
    pub(crate) fn inc_active_participants(&self) {
        self.active_participants.inc();
    }
    pub(crate) fn dec_active_participants(&self) {
        self.active_participants.dec();
    }

    #[cfg(feature = "active-speaker")]
    pub(crate) fn inc_dominant_speaker_changes(&self) {
        self.dominant_speaker_changes_total.inc();
    }

    pub(crate) fn update_peer_rtcp(&self, peer_id: u64, loss: f32, rtt_ms: f64, jitter_ms: f64) {
        let label = peer_id.to_string();
        let lv = &[label.as_str()];
        self.peer_loss_fraction
            .with_label_values(lv)
            .set(loss.into());
        self.peer_rtt_ms.with_label_values(lv).set(rtt_ms);
        self.peer_jitter_ms.with_label_values(lv).set(jitter_ms);
    }

    pub(crate) fn update_peer_bwe(&self, peer_id: u64, bps: u64) {
        let label = peer_id.to_string();
        self.bandwidth_estimate_bps
            .with_label_values(&[label.as_str()])
            .set(bps as f64);
    }

    #[cfg(feature = "active-speaker")]
    pub(crate) fn update_peer_speaker_scores(
        &self,
        peer_id: u64,
        immediate: f64,
        medium: f64,
        long_score: f64,
    ) {
        let label = peer_id.to_string();
        let lv = &[label.as_str()];
        self.speaker_immediate.with_label_values(lv).set(immediate);
        self.speaker_medium.with_label_values(lv).set(medium);
        self.speaker_long.with_label_values(lv).set(long_score);
    }

    /// Remove all per-peer label series for a disconnected peer.
    ///
    /// Safe to call with an unknown `peer_id` — `remove_label_values` returns
    /// `Err` which is silently ignored to avoid panics on double-reap.
    pub fn reap_dead_peer(&self, peer_id: u64) {
        let label = peer_id.to_string();
        let lv = &[label.as_str()];
        let _ = self.peer_loss_fraction.remove_label_values(lv);
        let _ = self.peer_rtt_ms.remove_label_values(lv);
        let _ = self.peer_jitter_ms.remove_label_values(lv);
        let _ = self.bandwidth_estimate_bps.remove_label_values(lv);
        let _ = self.speaker_immediate.remove_label_values(lv);
        let _ = self.speaker_medium.remove_label_values(lv);
        let _ = self.speaker_long.remove_label_values(lv);
    }
}

impl Default for SfuMetrics {
    fn default() -> Self {
        Self::new().expect("SfuMetrics::new at startup")
    }
}
