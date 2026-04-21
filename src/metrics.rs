//! Prometheus metrics for the SFU.
//!
//! One [`SfuMetrics`] per process, wrapped in [`Arc`][std::sync::Arc] and
//! threaded through constructors (no global statics).
//!
//! When the `metrics-prometheus` feature is **off**, all methods on
//! `SfuMetrics` are no-ops and the struct holds no Prometheus handles.
//! This lets `Client` and `Registry` always hold an `Arc<SfuMetrics>` with
//! no conditional compilation at call sites.

// ── feature = "metrics-prometheus" ──────────────────────────────────────────

#[cfg(feature = "metrics-prometheus")]
mod inner {
    use std::sync::Arc;

    use anyhow::Context;
    use prometheus::{IntCounter, IntCounterVec, IntGauge, Opts, Registry};

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
    }

    impl SfuMetrics {
        /// Construct a metrics instance, registering all handles into a new Prometheus registry.
        pub fn new() -> anyhow::Result<Self> {
            let registry =
                Registry::new_custom(Some("sfu".into()), None).context("create registry")?;

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
                    "Simulcast layer forwarding events by RID",
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

            Ok(Self {
                registry: Arc::new(registry),
                active_participants,
                forwarded_packets_total,
                layer_selection_total,
                dominant_speaker_changes_total,
                client_connect_total,
                client_disconnect_total,
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
    }

    impl Default for SfuMetrics {
        fn default() -> Self {
            Self::new().expect("SfuMetrics::new at startup")
        }
    }

    impl SfuMetrics {
        /// Infallible constructor for use at startup / in test helpers.
        pub fn new_default() -> Self {
            Self::default()
        }
    }
}

// ── feature = "metrics-prometheus" OFF ──────────────────────────────────────

#[cfg(not(feature = "metrics-prometheus"))]
mod inner {
    /// No-op metrics stub used when the `metrics-prometheus` feature is off.
    ///
    /// All methods compile away to nothing. `Client` and `Registry` always hold
    /// an `Arc<SfuMetrics>` without any `#[cfg]` at call sites.
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
    }
}

pub use inner::SfuMetrics;
