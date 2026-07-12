//! Per-subscriber state combining Kalman delay, loss, native GCC, and client hint.
//!
//! Ported from `oxpulse-partner-edge/crates/sfu/src/bandwidth/subscriber.rs`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

#[cfg(feature = "googcc-bwe")]
use super::googcc::GoogCcEstimator;
use super::kalman::DelayEstimator;
use super::loss::LossEstimator;

/// Initial bitrate assigned to a new subscriber (bps).
const INITIAL_BITRATE_BPS: f64 = 300_000.0;

/// Maximum age of a client-reported budget hint before it is discarded.
const CLIENT_HINT_MAX_AGE: Duration = Duration::from_secs(5);

/// Browser-reported bandwidth budget from DataChannel `{"type":"budget","bps":N}`.
#[derive(Debug, Clone, Copy)]
pub struct ClientHint {
    /// Budget ceiling in bits per second.
    pub bps: u64,
    /// Monotonic timestamp when the hint was received.
    pub received_at: Instant,
}

/// Which ceiling term produced the winning minimum in
/// [`PerSubscriber::combined_bps_with_term`] — i.e. which signal is currently
/// *binding* the combined bitrate estimate.
///
/// Additive observability seam (issue #2310 V0): a consumer can label a
/// Prometheus metric with the binding term without re-deriving the min-chain
/// or its private staleness window.
///
/// [`Self::GoogCc`] is only ever returned when the `googcc-bwe` feature is
/// enabled; the variant is defined unconditionally so downstream `match` and
/// metric-label mappings stay feature-agnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
// Open vocabulary: the min-chain terms evolve with the estimator (Arc 2 will
// retire GoogCc; future ceilings may be added). `#[non_exhaustive]` forces
// external `match` consumers to carry a wildcard arm so growth stays additive.
#[non_exhaustive]
pub enum BindingTerm {
    /// Kalman delay-based estimate won `min(delay, loss)`.
    Delay,
    /// Loss-based estimate won `min(delay, loss)`.
    Loss,
    /// Native str0m GCC ceiling (`native_estimate_bps`) bound the minimum.
    Native,
    /// GoogCC v2 ceiling bound the minimum (`googcc-bwe` feature only).
    GoogCc,
    /// A fresh browser budget hint (`client_hint`) bound the minimum.
    ClientHint,
}

impl BindingTerm {
    /// Every variant, so consumers can pre-touch all metric-label series
    /// exhaustively without hardcoding the vocabulary.
    pub const ALL: [BindingTerm; 5] = [
        BindingTerm::Delay,
        BindingTerm::Loss,
        BindingTerm::Native,
        BindingTerm::GoogCc,
        BindingTerm::ClientHint,
    ];

    /// Stable snake_case label for the Prometheus `term` label
    /// (`delay` | `loss` | `native` | `googcc` | `client_hint`).
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            BindingTerm::Delay => "delay",
            BindingTerm::Loss => "loss",
            BindingTerm::Native => "native",
            BindingTerm::GoogCc => "googcc",
            BindingTerm::ClientHint => "client_hint",
        }
    }
}

/// Reproduce the [`f64::min`] ceiling chain of [`PerSubscriber::combined_bps`]
/// while tracking which term produced the running minimum (the *binding* term).
///
/// Kept as a free function so the min-chain formula can be unit-tested against
/// arbitrary term values — including NaN and exact ties — independently of the
/// estimator state that is otherwise awkward to construct.
///
/// NaN handling mirrors [`f64::min`] exactly (NaN-discarding): a NaN candidate
/// never displaces a non-NaN running minimum, and a NaN running minimum is
/// replaced by the next candidate. This keeps the delegating
/// [`PerSubscriber::combined_bps`] numerically bit-identical to the original
/// hand-written chain even under a pathological NaN estimate.
fn combine_terms(
    delay: f64,
    loss: f64,
    native: Option<f64>,
    googcc: Option<f64>,
    hint: Option<f64>,
) -> (f64, BindingTerm) {
    // Reproduce `best = f64::min(best, cand)` with argmin tracking: take the
    // candidate when it is strictly smaller, OR when the running minimum is NaN
    // (`f64::min` returns the non-NaN operand).
    fn min_update(best: &mut f64, term: &mut BindingTerm, cand: f64, cand_term: BindingTerm) {
        if best.is_nan() || cand < *best {
            *best = cand;
            *term = cand_term;
        }
    }

    let mut best = delay;
    let mut term = BindingTerm::Delay;
    min_update(&mut best, &mut term, loss, BindingTerm::Loss);
    if let Some(n) = native {
        min_update(&mut best, &mut term, n, BindingTerm::Native);
    }
    if let Some(g) = googcc {
        min_update(&mut best, &mut term, g, BindingTerm::GoogCc);
    }
    if let Some(h) = hint {
        min_update(&mut best, &mut term, h, BindingTerm::ClientHint);
    }
    (best.max(0.0), term)
}

/// Per-subscriber BWE state: delay estimate, loss estimate, native GCC ceiling,
/// browser hint ceiling, and send-time map for TWCC gradient computation.
#[derive(Debug)]
pub struct PerSubscriber {
    /// Map from extended RTP seq number -> send Instant, used to compute
    /// inter-send deltas for the TWCC gradient.
    pub send_times: HashMap<u64, Instant>,
    /// Arrival time of the last successfully received packet for gradient delta.
    pub last_arrival: Option<Instant>,
    /// Send time corresponding to `last_arrival` (i.e. the send time of the last
    /// *received* packet). Tracked separately from `send_times` so that a lost
    /// packet does not corrupt the inter-send delta on the next received packet.
    pub last_send_for_received: Option<Instant>,
    /// Kalman-filtered delay-based rate estimator.
    pub delay: DelayEstimator,
    /// Loss-window-based rate estimator.
    pub loss: LossEstimator,
    /// Estimated round-trip time (from RTCP SR/RR, if available).
    pub rtt: Option<Duration>,
    /// Native GCC estimate from str0m EgressBitrateEstimate event (ceiling).
    pub native_estimate_bps: Option<f64>,
    /// Browser-reported budget hint (additional ceiling, expires after 5s).
    pub client_hint: Option<ClientHint>,
    /// Per-subscriber GoogCC v2 estimator. **None by default** — must be
    /// initialised by the consumer at subscriber creation:
    ///
    /// ```ignore
    /// use oxpulse_sfu_kit::bwe::googcc::GoogCcEstimator;
    /// subscriber.googcc = Some(GoogCcEstimator::new());
    /// ```
    ///
    /// When `Some`, [`Self::combined_bps`] applies `googcc.current_bps()` as
    /// an additional ceiling alongside the Kalman estimate. Feed via
    /// `googcc.on_receive(arrival_ms, send_ms, loss)` from the TWCC feedback
    /// handler (typically in `BandwidthEstimator::on_twcc_feedback`).
    ///
    /// See [`crate::bwe::googcc`] module docs for the full integration recipe.
    #[cfg(feature = "googcc-bwe")]
    pub googcc: Option<GoogCcEstimator>,
}

impl PerSubscriber {
    /// Create new subscriber state at INITIAL_BITRATE_BPS.
    pub fn new() -> Self {
        Self {
            send_times: HashMap::new(),
            last_arrival: None,
            last_send_for_received: None,
            delay: DelayEstimator::new(INITIAL_BITRATE_BPS),
            loss: LossEstimator::new(INITIAL_BITRATE_BPS),
            rtt: None,
            native_estimate_bps: None,
            client_hint: None,
            #[cfg(feature = "googcc-bwe")]
            googcc: None,
        }
    }

    /// Whether a GoogCC estimator is wired in for this subscriber.
    ///
    /// `false` means [`Self::combined_bps`] silently omits the GoogCC ceiling for
    /// this subscriber — a presence signal that a normal-looking `estimate_bps`
    /// value otherwise hides. Aggregate via [`super::estimator::BandwidthEstimator::googcc_coverage`].
    #[cfg(feature = "googcc-bwe")]
    #[must_use]
    pub fn googcc_active(&self) -> bool {
        self.googcc.is_some()
    }

    /// Combined bitrate estimate: min(kalman, loss) then apply GCC and hint ceilings.
    ///
    /// Returns at least 0; the result is not further clamped to MIN_BITRATE_BPS
    /// here so callers can distinguish "no estimate yet" from a floor-constrained value.
    ///
    /// Delegates to [`Self::combined_bps_with_term`] so the value and the
    /// binding-term label are computed by one function body and can never
    /// drift; the numeric result is unchanged from the original hand-written
    /// min-chain (see the `combine_terms` NaN-equivalence tests).
    #[must_use]
    pub fn combined_bps(&self, now: Instant) -> f64 {
        self.combined_bps_with_term(now).0
    }

    /// Combined bitrate estimate *and* the [`BindingTerm`] that produced the
    /// winning minimum this instant.
    ///
    /// The `f64` component is bit-identical to [`Self::combined_bps`] (which
    /// delegates here). The term identifies which ceiling — Kalman delay, loss,
    /// native GCC, GoogCC v2, or the browser client hint — is currently binding
    /// the estimate, without changing the estimate or the existing
    /// [`Self::combined_bps`] contract.
    ///
    /// On an exact `f64` tie the earliest term in min-application order
    /// (`delay` → `loss` → `native` → `googcc` → `client_hint`) is reported —
    /// e.g. at boot with `delay == loss == 300k` the term is
    /// [`BindingTerm::Delay`]. Exact ties between continuous estimates are
    /// effectively unreachable outside that boot state.
    ///
    /// V0 observability seam (issue #2310): a subscriber whose forwarding
    /// estimate is pinned low by a still-fresh `client_hint` reports
    /// [`BindingTerm::ClientHint`] every tick with no recovery.
    #[must_use]
    pub fn combined_bps_with_term(&self, now: Instant) -> (f64, BindingTerm) {
        let native = self.native_estimate_bps;

        #[cfg(feature = "googcc-bwe")]
        let googcc = self.googcc.as_ref().map(|gcc| gcc.current_bps() as f64);
        #[cfg(not(feature = "googcc-bwe"))]
        let googcc: Option<f64> = None;

        let hint = match self.client_hint {
            Some(h) if now.duration_since(h.received_at) < CLIENT_HINT_MAX_AGE => {
                Some(h.bps as f64)
            }
            _ => None,
        };

        combine_terms(
            self.delay.bitrate_bps(),
            self.loss.bitrate_bps(),
            native,
            googcc,
            hint,
        )
    }
}

impl Default for PerSubscriber {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn combined_bps_takes_minimum_of_delay_and_loss() {
        let now = Instant::now();
        let mut sub = PerSubscriber::new();
        sub.delay = DelayEstimator::new(2_000_000.0);
        sub.loss = LossEstimator::new(1_000_000.0);
        let combined = sub.combined_bps(now);
        // Should take the minimum ~1_000_000
        assert!(
            combined <= 1_100_000.0,
            "expected approx 1Mbps (min of 2M and 1M), got {combined}"
        );
    }

    #[test]
    fn native_estimate_acts_as_ceiling() {
        let now = Instant::now();
        let mut sub = PerSubscriber::new();
        sub.delay = DelayEstimator::new(2_000_000.0);
        sub.loss = LossEstimator::new(2_000_000.0);
        sub.native_estimate_bps = Some(500_000.0);
        let combined = sub.combined_bps(now);
        assert!(
            combined <= 500_100.0,
            "native GCC ceiling not applied: {combined}"
        );
    }

    #[test]
    fn client_hint_acts_as_ceiling_when_fresh() {
        let now = Instant::now();
        let mut sub = PerSubscriber::new();
        sub.delay = DelayEstimator::new(2_000_000.0);
        sub.loss = LossEstimator::new(2_000_000.0);
        sub.client_hint = Some(ClientHint {
            bps: 400_000,
            received_at: now,
        });
        let combined = sub.combined_bps(now);
        assert!(
            combined <= 400_100.0,
            "client hint ceiling not applied: {combined}"
        );
    }

    #[test]
    fn stale_client_hint_is_ignored() {
        let past = Instant::now() - Duration::from_secs(10); // older than CLIENT_HINT_MAX_AGE
        let now = Instant::now();
        let mut sub = PerSubscriber::new();
        sub.delay = DelayEstimator::new(2_000_000.0);
        sub.loss = LossEstimator::new(2_000_000.0);
        sub.client_hint = Some(ClientHint {
            bps: 100,
            received_at: past,
        }); // absurdly small
        let combined = sub.combined_bps(now);
        // Hint is stale -> should not cap at 100 bps
        assert!(
            combined > 1_000.0,
            "stale client hint should be ignored, got {combined}"
        );
    }

    // ── BindingTerm / combined_bps_with_term ─────────────────────────────────

    /// Independent re-implementation of the ORIGINAL hand-written min-chain, to
    /// falsify the refactored `combine_terms` against it (anti-vacuous: this is
    /// NOT a copy of the new code path — it is the pre-refactor formula).
    fn old_min_chain(
        delay: f64,
        loss: f64,
        native: Option<f64>,
        googcc: Option<f64>,
        hint: Option<f64>,
    ) -> f64 {
        let base = delay.min(loss);
        let after_native = match native {
            Some(n) => base.min(n),
            None => base,
        };
        let after_gcc = match googcc {
            Some(g) => after_native.min(g),
            None => after_native,
        };
        let after_hint = match hint {
            Some(h) => after_gcc.min(h),
            None => after_gcc,
        };
        after_hint.max(0.0)
    }

    #[test]
    fn combine_terms_value_matches_original_min_chain_including_nan() {
        // Representative term values: normal, high, NaN (pathological Kalman
        // output), and 0. The final `.max(0.0)` clamps NaN → 0.0 in BOTH the
        // original chain and combine_terms, so plain `==` is valid (no result
        // is ever NaN).
        let scalars = [30_000.0_f64, 50_000.0, f64::NAN, 0.0];
        let opts = [None, Some(40_000.0_f64), Some(f64::NAN), Some(0.0)];
        for &delay in &scalars {
            for &loss in &scalars {
                for &native in &opts {
                    for &googcc in &opts {
                        for &hint in &opts {
                            let (got, term) = combine_terms(delay, loss, native, googcc, hint);
                            let want = old_min_chain(delay, loss, native, googcc, hint);
                            assert_eq!(
                                got, want,
                                "value drift: delay={delay} loss={loss} native={native:?} \
                                 googcc={googcc:?} hint={hint:?} term={term:?}"
                            );
                            assert!(
                                !got.is_nan(),
                                "combine_terms must never return NaN (clamped by max(0.0))"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn binding_term_is_delay_or_loss_at_boot() {
        // Fresh subscriber: delay and loss both boot at INITIAL_BITRATE_BPS, no
        // ceilings. Strict-`<` argmin keeps the earlier term → Delay.
        let now = Instant::now();
        let sub = PerSubscriber::new();
        let (_, term) = sub.combined_bps_with_term(now);
        assert_eq!(term, BindingTerm::Delay, "boot state should report delay");
    }

    #[test]
    fn binding_term_loss_when_loss_lower() {
        let now = Instant::now();
        let mut sub = PerSubscriber::new();
        sub.delay = DelayEstimator::new(2_000_000.0);
        sub.loss = LossEstimator::new(1_000_000.0);
        let (bps, term) = sub.combined_bps_with_term(now);
        assert_eq!(term, BindingTerm::Loss);
        assert_eq!(bps, sub.combined_bps(now), "value must match combined_bps");
    }

    #[test]
    fn binding_term_native_when_native_ceiling_lowest() {
        let now = Instant::now();
        let mut sub = PerSubscriber::new();
        sub.delay = DelayEstimator::new(2_000_000.0);
        sub.loss = LossEstimator::new(2_000_000.0);
        sub.native_estimate_bps = Some(500_000.0);
        let (_, term) = sub.combined_bps_with_term(now);
        assert_eq!(term, BindingTerm::Native);
    }

    #[test]
    fn binding_term_client_hint_when_fresh_hint_lowest() {
        let now = Instant::now();
        let mut sub = PerSubscriber::new();
        sub.delay = DelayEstimator::new(2_000_000.0);
        sub.loss = LossEstimator::new(2_000_000.0);
        sub.native_estimate_bps = Some(800_000.0);
        sub.client_hint = Some(ClientHint {
            bps: 400_000,
            received_at: now,
        });
        let (_, term) = sub.combined_bps_with_term(now);
        assert_eq!(term, BindingTerm::ClientHint);
    }

    #[test]
    fn binding_term_not_client_hint_when_stale() {
        // A stale hint (older than CLIENT_HINT_MAX_AGE) that would otherwise
        // bind must be ignored — the term falls back to the native ceiling.
        let past = Instant::now() - Duration::from_secs(10);
        let now = Instant::now();
        let mut sub = PerSubscriber::new();
        sub.delay = DelayEstimator::new(2_000_000.0);
        sub.loss = LossEstimator::new(2_000_000.0);
        sub.native_estimate_bps = Some(600_000.0);
        sub.client_hint = Some(ClientHint {
            bps: 100,
            received_at: past,
        });
        let (_, term) = sub.combined_bps_with_term(now);
        assert_eq!(
            term,
            BindingTerm::Native,
            "stale hint must not be the binding term"
        );
    }

    #[test]
    fn binding_term_as_str_and_all_cover_every_variant() {
        assert_eq!(BindingTerm::ALL.len(), 5);
        let labels: Vec<&str> = BindingTerm::ALL.iter().map(BindingTerm::as_str).collect();
        assert_eq!(
            labels,
            vec!["delay", "loss", "native", "googcc", "client_hint"]
        );
    }
}

#[cfg(all(test, feature = "googcc-bwe"))]
mod googcc_integration_tests {
    use super::*;
    use crate::bwe::googcc::GoogCcEstimator;

    #[test]
    fn googcc_acts_as_ceiling_when_set() {
        let now = Instant::now();
        let mut sub = PerSubscriber::new();
        // Force Kalman/loss high
        sub.delay = DelayEstimator::new(5_000_000.0);
        sub.loss = LossEstimator::new(5_000_000.0);
        // GoogCC at a lower estimate
        let mut gcc = GoogCcEstimator::new();
        gcc.force_bps_for_tests(300_000);
        sub.googcc = Some(gcc);

        let combined = sub.combined_bps(now);
        assert!(
            combined <= 300_100.0,
            "GoogCC ceiling not applied: {combined}"
        );
    }

    #[test]
    fn googcc_none_does_not_affect_combined() {
        let now = Instant::now();
        let mut sub = PerSubscriber::new();
        sub.delay = DelayEstimator::new(1_000_000.0);
        sub.loss = LossEstimator::new(1_000_000.0);
        // googcc = None (default)
        let combined = sub.combined_bps(now);
        // Should be ~1Mbps (from delay/loss), not artificially capped
        assert!(
            combined > 900_000.0,
            "missing GoogCC should not cap estimate: {combined}"
        );
    }

    #[test]
    fn binding_term_googcc_when_googcc_ceiling_lowest() {
        let now = Instant::now();
        let mut sub = PerSubscriber::new();
        sub.delay = DelayEstimator::new(5_000_000.0);
        sub.loss = LossEstimator::new(5_000_000.0);
        sub.native_estimate_bps = Some(2_000_000.0);
        let mut gcc = GoogCcEstimator::new();
        gcc.force_bps_for_tests(300_000);
        sub.googcc = Some(gcc);
        let (_, term) = sub.combined_bps_with_term(now);
        assert_eq!(term, BindingTerm::GoogCc);
    }
}
