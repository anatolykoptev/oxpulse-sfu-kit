//! Prometheus-backed [`SfuMetrics`] implementation.
//!
//! Active when the `metrics-prometheus` feature is enabled.

use std::sync::Arc;

use anyhow::Context;
use prometheus::{
    GaugeVec, HistogramOpts, HistogramVec, IntCounter, IntCounterVec, IntGauge, Opts, Registry,
};

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
    /// Counter — pacer suspend-video transitions. Label `direction` ∈ {"enter", "exit"}.
    /// `enter` increments on `PacerAction::SuspendVideo` (BWE crossed below
    /// `SUSPEND_VIDEO_BPS`); `exit` increments on `PacerAction::RestoreAudio`
    /// (BWE recovered above `AUDIO_ONLY_BPS`). Phase 7 of the 1 KB/s resilience plan.
    pacer_suspend_video_total: prometheus::IntCounterVec,
    /// Per-subscriber drop count. Label `peer_id` reaped on disconnect via
    /// [`SfuMetrics::reap_dead_peer`] to bound cardinality across reconnect
    /// churn. Mirrors the F2b-2 reap pattern from partner-edge. Audio frames
    /// are not counted (they are forwarded in suspended state).
    video_frames_dropped_total: prometheus::IntCounterVec,
    /// End-to-end RTP forwarding latency: time from packet receive at the
    /// publisher side (`SfuMediaPayload::network_time`) to handoff to the
    /// subscriber's str0m writer. Label `media_kind` ∈ {"audio", "video", "other"}.
    ///
    /// Buckets (seconds): 100µs, 500µs, 1ms, 5ms, 10ms, 50ms, 100ms.
    /// Packets taking longer than 100ms land in the +Inf bucket and are a
    /// signal of degraded call quality worth alerting on.
    pub forward_latency_seconds: HistogramVec,
    /// RTP bytes by direction and media kind.
    ///
    /// `direction="in"` counts bytes received from publishers (inbound to the SFU).
    /// `direction="out"` counts bytes forwarded to subscribers (outbound from the SFU).
    /// `kind` ∈ {"audio", "video"}.
    ///
    /// Prometheus query examples:
    /// - `rate(sfu_track_bytes_total{direction="out",kind="video"}[1m])` — outbound video throughput
    /// - `rate(sfu_track_bytes_total{direction="in"}[1m])` — total inbound throughput
    pub track_bytes_total: IntCounterVec,
    /// RTCP PLI (Picture Loss Indication) events.
    ///
    /// Uses kit-side semantic vocabulary (matching `sfu_track_bytes_total`):
    /// `direction="in"` increments when a subscriber sends a PLI to the SFU
    /// (keyframe request received by the kit from a subscriber).
    /// `direction="out"` increments when the SFU sends a PLI upstream to a publisher
    /// (non-contiguous media detected on an incoming track).
    ///
    /// Prometheus query examples:
    /// - `rate(sfu_rtcp_pli_total{direction="in"}[1m])` — subscriber PLI rate (video corruption proxy)
    /// - `rate(sfu_rtcp_pli_total{direction="out"}[1m])` — publisher keyframe request rate
    pub rtcp_pli_total: IntCounterVec,
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

        let pacer_suspend_video_total = reg!(IntCounterVec::new(
            Opts::new(
                "pacer_suspend_video_total",
                "Pacer suspend-video transitions (label `direction` = enter | exit).",
            ),
            &["direction"],
        )
        .context("pacer_suspend_video_total")?);

        let video_frames_dropped_total = reg!(IntCounterVec::new(
            Opts::new(
                "video_frames_dropped_total",
                "Outbound video frames dropped because the subscriber pacer is suspended. \
                 Label peer_id reaped on disconnect.",
            ),
            &["peer_id"],
        )
        .context("video_frames_dropped_total")?);

        let forward_latency_seconds = reg!(HistogramVec::new(
            HistogramOpts::new(
                "forward_latency_seconds",
                "End-to-end RTP forwarding latency: from packet receive at the publisher \
                 to handoff to the subscriber str0m writer. \
                 Label media_kind = audio | video | other.",
            )
            .buckets(vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1]),
            &["media_kind"],
        )
        .context("forward_latency_seconds")?);

        let track_bytes_total = reg!(IntCounterVec::new(
            Opts::new(
                "track_bytes_total",
                "RTP payload bytes by direction (in=publisher→SFU, out=SFU→subscriber) and kind.",
            ),
            &["direction", "kind"],
        )
        .context("track_bytes_total")?);

        let rtcp_pli_total = reg!(IntCounterVec::new(
            Opts::new(
                "rtcp_pli_total",
                "RTCP PLI events. direction=in: received from subscriber; direction=out: sent to publisher.",
            ),
            &["direction"],
        )
        .context("rtcp_pli_total")?);

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
            pacer_suspend_video_total,
            video_frames_dropped_total,
            forward_latency_seconds,
            track_bytes_total,
            rtcp_pli_total,
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

    /// Observe end-to-end forwarding latency for a single RTP packet.
    ///
    /// `kind` must be one of `"audio"`, `"video"`, or `"other"`.
    /// `elapsed_secs` is the number of seconds from packet receipt to subscriber handoff.
    /// This is alloc-free: `Histogram::observe` does a single atomic bucket increment.
    pub(crate) fn observe_forward_latency(&self, kind: &str, elapsed_secs: f64) {
        self.forward_latency_seconds
            .with_label_values(&[kind])
            .observe(elapsed_secs);
    }

    pub(crate) fn inc_layer_selection(&self, layer: &str) {
        self.layer_selection_total.with_label_values(&[layer]).inc();
    }

    pub(crate) fn inc_suspend_video(&self, direction: &str) {
        self.pacer_suspend_video_total
            .with_label_values(&[direction])
            .inc();
    }

    /// Pre-resolve the per-peer drop counter so the fanout hot path avoids a
    /// per-frame `to_string()` alloc. Call once at peer admit; the returned
    /// handle is a cheap atomic-ref-counted pointer into the underlying
    /// `IntCounterVec`. Bounded cardinality: reaped via [`Self::reap_dead_peer`].
    pub(crate) fn peer_drop_counter(&self, peer_id: u64) -> prometheus::IntCounter {
        let label = peer_id.to_string();
        self.video_frames_dropped_total
            .with_label_values(&[label.as_str()])
    }

    /// Remove the `video_frames_dropped_total{peer_id=…}` series for a
    /// disconnected peer.  Safe to call with an unknown `peer_id`.
    pub(crate) fn reap_video_frames_dropped(&self, peer_id: u64) {
        let label = peer_id.to_string();
        let _ = self
            .video_frames_dropped_total
            .remove_label_values(&[label.as_str()]);
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

    /// Add `n` bytes to the inbound byte counter for a given media kind.
    ///
    /// Call once per inbound `MediaData` event in `track_in_media`.
    /// `kind` must be `"audio"` or `"video"`.
    pub(crate) fn add_track_bytes_in(&self, kind: &str, n: u64) {
        self.track_bytes_total
            .with_label_values(&["in", kind])
            .inc_by(n);
    }

    /// Add `n` bytes to the outbound byte counter for a given media kind.
    ///
    /// Call once per forwarded `MediaData` packet in `handle_media_data_out`.
    /// `kind` must be `"audio"` or `"video"`.
    pub(crate) fn add_track_bytes_out(&self, kind: &str, n: u64) {
        self.track_bytes_total
            .with_label_values(&["out", kind])
            .inc_by(n);
    }

    /// Increment the PLI counter for the given direction.
    ///
    /// `direction` must be `"in"` (received from subscriber) or `"out"` (sent to publisher).
    /// Uses kit-side semantic vocabulary — matches `sfu_track_bytes_total` direction labels.
    pub(crate) fn inc_rtcp_pli(&self, direction: &str) {
        self.rtcp_pli_total.with_label_values(&[direction]).inc();
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
        self.reap_video_frames_dropped(peer_id);
    }
}

impl Default for SfuMetrics {
    fn default() -> Self {
        Self::new().expect("SfuMetrics::new at startup")
    }
}
