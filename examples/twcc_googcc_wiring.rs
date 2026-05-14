//! TWCC → GoogCC wiring example.
//!
//! Shows how to convert raw TWCC feedback into the absolute-monotonic-ms
//! timestamps that [`oxpulse_sfu_kit::bwe::googcc::GoogCcEstimator::on_receive`]
//! expects, and how to wire a [`oxpulse_sfu_kit::bwe::subscriber::PerSubscriber`]
//! for combined BWE.
//!
//! Run with:
//! ```not_rust
//! cargo run --example twcc_googcc_wiring --features googcc-bwe,kalman-bwe,test-utils
//! ```

use std::time::{Duration, Instant};

use oxpulse_sfu_kit::bwe::googcc::GoogCcEstimator;
use oxpulse_sfu_kit::bwe::subscriber::PerSubscriber;

fn main() {
    println!("=== TWCC -> GoogCC wiring example ===\n");

    // -----------------------------------------------------------------------
    // Part 1: timestamp conversion
    //
    // TWCC feedback arrives as a *relative* receive-time delta per packet
    // (typically in 250 µs units per RFC 8888).  GoogCcEstimator::on_receive
    // requires *absolute* monotonic milliseconds, so we integrate:
    //
    //   arrival_ms[n] = arrival_ms[n-1] + recv_delta_ms
    //   send_ms[n]    = send_ms[n-1]    + send_delta_ms
    // -----------------------------------------------------------------------

    // Simulate 30 TWCC packets with stable 20 ms inter-packet interval.
    let mut gcc = GoogCcEstimator::new();

    let mut arrival_ms = 0.0_f64;
    let mut send_ms = 0.0_f64;
    let interval_ms = 20.0;

    for _i in 0..30 {
        // Both deltas equal -> no delay build-up -> Normal / Underuse state.
        let recv_delta_ms = interval_ms;
        let send_delta_ms = interval_ms;

        arrival_ms += recv_delta_ms;
        send_ms += send_delta_ms;

        let bps = gcc.on_receive(arrival_ms, send_ms, 0.001); // 0.1% loss
        let _ = bps; // use in real code to drive forwarding rate
    }
    println!(
        "After 30 stable packets:  bps = {} (initial was 500_000)",
        gcc.current_bps()
    );
    assert!(
        gcc.current_bps() > 500_000,
        "stable link should grow bitrate"
    );

    // -----------------------------------------------------------------------
    // Part 2: integrating GoogCC into PerSubscriber
    // -----------------------------------------------------------------------

    let mut sub = PerSubscriber::new();
    sub.googcc = Some(GoogCcEstimator::new());

    // Simulate TWCC feedback loop:
    //  - sub.send_times records send_instant per extended seq
    //  - TWCC feedback provides recv_delta per packet
    // Here we just drive it directly for illustration.
    let base = Instant::now();
    let mut last_arrival: Option<Instant> = None;
    let mut last_send: Option<Instant> = None;

    for i in 0..30_u64 {
        let send_instant = base + Duration::from_millis(i * 20);
        let recv_instant = send_instant + Duration::from_millis(2); // 2 ms propagation

        if let (Some(la), Some(ls)) = (last_arrival, last_send) {
            let arr_delta_ms = recv_instant.duration_since(la).as_secs_f64() * 1000.0;
            let snd_delta_ms = send_instant.duration_since(ls).as_secs_f64() * 1000.0;

            // Convert to absolute ms by accumulating.
            // (In production this accumulation lives in BandwidthEstimator.)
            let arr_abs_ms = arr_delta_ms * (i as f64);
            let snd_abs_ms = snd_delta_ms * (i as f64);

            if let Some(gcc) = sub.googcc.as_mut() {
                gcc.on_receive(arr_abs_ms, snd_abs_ms, 0.001);
            }
        }
        last_arrival = Some(recv_instant);
        last_send = Some(send_instant);
    }

    let combined = sub.combined_bps(Instant::now());
    println!("PerSubscriber combined_bps = {combined:.0}");
    assert!(combined > 0.0);

    println!("\nDone. See src/bwe/googcc/ and src/bwe/subscriber.rs for details.");
}
