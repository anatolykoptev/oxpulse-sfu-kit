//! TWCC feedback ingestion: computes inter-arrival delay gradients and feeds
//! them into the Kalman filter and loss estimator.
//!
//! Ported from `oxpulse-partner-edge/crates/sfu/src/bandwidth/feedback.rs`.

use super::subscriber::PerSubscriber;
use std::time::Instant;

/// A single packet record from a TWCC feedback report.
#[derive(Debug, Clone)]
pub struct TwccSample {
    /// Extended RTP sequence number (u16 extended to u64 monotonically by the caller).
    pub seq: u64,
    /// Arrival time at the receiver; `None` if the packet was lost.
    pub arrival: Option<Instant>,
}

/// A parsed TWCC feedback batch for one subscriber tick.
pub struct TwccFeedback {
    /// Ordered list of samples from the TWCC report (ascending seq).
    pub samples: Vec<TwccSample>,
}

/// Feed a full TWCC feedback batch into a subscriber's BWE state.
///
/// For each sample:
/// - Records loss status in `PerSubscriber::loss`.
/// - If the packet was received and a send-time exists, computes the
///   inter-arrival gradient and feeds it into `PerSubscriber::delay` (Kalman).
///
/// The gradient is computed only against the previous *received* packet's
/// send-time (tracked in `PerSubscriber::last_send_for_received`), not against
/// whatever packet happened to precede this one in the sequence.  This prevents
/// a lost packet from introducing a phantom congestion signal on the next
/// received packet.
///
/// After processing all samples, applies rate control on both estimators.
pub fn ingest_twcc(sub: &mut PerSubscriber, feedback: &TwccFeedback, now: Instant) {
    for sample in &feedback.samples {
        // Record received/lost for the loss estimator.
        sub.loss.record(sample.arrival.is_some());

        if let Some(arrival) = sample.arrival {
            if let Some(send_t) = sub.send_times.get(&sample.seq).copied() {
                // Compute inter-arrival gradient only when we have a previous
                // *received* packet to compare against.  Using last_send_for_received
                // (not a prev_seq lookup) ensures a lost packet cannot corrupt the
                // inter-send delta for the following received packet.
                if let (Some(prev_arr), Some(prev_send)) =
                    (sub.last_arrival, sub.last_send_for_received)
                {
                    let recv_delta_us = arrival.duration_since(prev_arr).as_micros() as f64;
                    let send_delta_us = send_t.duration_since(prev_send).as_micros() as f64;
                    let gradient_us = recv_delta_us - send_delta_us;
                    sub.delay.update_kalman(gradient_us);
                }
                sub.last_arrival = Some(arrival);
                sub.last_send_for_received = Some(send_t);

                // Update RTT estimate: RTT ≈ 2 × one-way delay (crude but useful).
                let one_way = arrival.duration_since(send_t);
                sub.rtt = Some(one_way * 2);
            }
        }
    }

    // Apply rate control after the full batch.
    sub.delay.apply_rate_control(now);
    sub.loss.apply_rate_control(now);
}

#[cfg(test)]
mod tests {
    use super::super::subscriber::PerSubscriber;
    use super::*;
    use std::time::{Duration, Instant};

    fn make_send_times(base: Instant, seqs: &[u64], interval_ms: u64) -> PerSubscriber {
        let mut sub = PerSubscriber::new();
        for (i, &seq) in seqs.iter().enumerate() {
            sub.send_times
                .insert(seq, base + Duration::from_millis(i as u64 * interval_ms));
        }
        sub
    }

    #[test]
    fn positive_gradient_feeds_kalman() {
        let base = Instant::now();
        let seqs = [1u64, 2, 3, 4, 5];
        let mut sub = make_send_times(base, &seqs, 10); // 10ms send interval

        // Packets arrive with increasing delay (+5ms per packet after first)
        let feedback = TwccFeedback {
            samples: seqs
                .iter()
                .enumerate()
                .map(|(i, &seq)| TwccSample {
                    seq,
                    // seq 1 arrives at +15ms; each subsequent arrives +15ms later than expected
                    arrival: Some(base + Duration::from_millis(15 + i as u64 * 15)),
                })
                .collect(),
        };

        let before = sub.delay.filtered_gradient_us();
        ingest_twcc(&mut sub, &feedback, base + Duration::from_millis(90));
        let after = sub.delay.filtered_gradient_us();
        // Growing inter-arrival delay -> positive gradient (congestion signal)
        assert!(
            after > before || after > 0.0,
            "expected Kalman to pick up positive gradient; before={before}, after={after}"
        );
    }

    #[test]
    fn loss_recorded_for_missing_packets() {
        let base = Instant::now();
        let mut sub = PerSubscriber::new();
        // seq 1 sent
        sub.send_times.insert(1u64, base);
        sub.send_times
            .insert(2u64, base + Duration::from_millis(10));

        let feedback = TwccFeedback {
            samples: vec![
                TwccSample {
                    seq: 1,
                    arrival: Some(base + Duration::from_millis(15)),
                },
                TwccSample {
                    seq: 2,
                    arrival: None,
                }, // lost
            ],
        };

        ingest_twcc(&mut sub, &feedback, base + Duration::from_millis(30));
        assert!(
            sub.loss.loss_fraction() > 0.0,
            "lost packet should be recorded"
        );
    }

    #[test]
    fn zero_gradient_on_uniform_spacing() {
        let base = Instant::now();
        let seqs = [10u64, 11, 12, 13, 14];
        let mut sub = make_send_times(base, &seqs, 10); // 10ms send interval

        // Perfect uniform arrival: same 10ms spacing -> gradient ~0
        let feedback = TwccFeedback {
            samples: seqs
                .iter()
                .enumerate()
                .map(|(i, &seq)| TwccSample {
                    seq,
                    arrival: Some(base + Duration::from_millis(20 + i as u64 * 10)),
                })
                .collect(),
        };

        ingest_twcc(&mut sub, &feedback, base + Duration::from_millis(70));
        let gradient = sub.delay.filtered_gradient_us();
        // Should be close to 0 (within Kalman noise)
        assert!(
            gradient.abs() < 5_000.0,
            "uniform spacing should give ~0 gradient, got {gradient}"
        );
    }

    /// Verify that a lost packet in the middle of a batch does not corrupt the
    /// inter-send delta on the packet that follows it.
    ///
    /// Send spacing: uniform 10 ms.  seq 3 is lost; seqs 1,2,4,5 arrive with
    /// the same 10 ms uniform spacing (no congestion).  The Kalman gradient
    /// should stay near zero because we compare arrival of seq 4 against seq 2
    /// (the last *received* packet), not against the lost seq 3.
    #[test]
    fn loss_in_middle_does_not_corrupt_gradient() {
        let base = Instant::now();
        let seqs = [1u64, 2, 3, 4, 5];
        let mut sub = make_send_times(base, &seqs, 10);

        // Seq 3 lost; the rest arrive with uniform 10ms spacing
        // send:   0  10  20  30  40 ms
        // arrive: 5  15  -   35  45 ms  (offset +5ms constant, spacing 10ms)
        let feedback = TwccFeedback {
            samples: vec![
                TwccSample {
                    seq: 1,
                    arrival: Some(base + Duration::from_millis(5)),
                },
                TwccSample {
                    seq: 2,
                    arrival: Some(base + Duration::from_millis(15)),
                },
                TwccSample {
                    seq: 3,
                    arrival: None,
                }, // lost
                TwccSample {
                    seq: 4,
                    arrival: Some(base + Duration::from_millis(35)),
                },
                TwccSample {
                    seq: 5,
                    arrival: Some(base + Duration::from_millis(45)),
                },
            ],
        };

        ingest_twcc(&mut sub, &feedback, base + Duration::from_millis(60));
        let gradient = sub.delay.filtered_gradient_us();
        // If the bug were present, seq4 would compute recv_delta=35-15=20ms but
        // send_delta=30-20=10ms -> gradient=+10ms phantom congestion.
        // With the fix, recv_delta=35-15=20ms, send_delta=30-10=20ms -> gradient=0.
        assert!(
            gradient.abs() < 5_000.0,
            "lost packet should not corrupt gradient; got {gradient} us"
        );
    }
}
