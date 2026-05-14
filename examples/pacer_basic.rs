//! Basic pacer example.
//!
//! Demonstrates how to drive `SubscriberPacer` with simulated BWE readings
//! and how to use `PacerConfig` to tune thresholds for your deployment.
//!
//! Run with:
//! ```not_rust
//! cargo run --example pacer_basic --features pacer
//! ```

use oxpulse_sfu_kit::bwe::PacerConfig;
use oxpulse_sfu_kit::{PacerAction, SubscriberPacer};

fn main() {
    println!("=== SubscriberPacer basic example ===\n");

    // --- Default config ---
    println!("1. Default config (production-tuned thresholds):");
    let mut pacer = SubscriberPacer::new();

    let readings: &[(u64, &str)] = &[
        (800_000, "HIGH link"),
        (800_000, "HIGH link"),
        (800_000, "HIGH link"),
        (400_000, "MEDIUM link"),
        (400_000, "MEDIUM link"),
        (400_000, "MEDIUM link"),
        (50_000, "LOW (audio only)"),
        (200_000, "MEDIUM recovering"),
    ];

    for (bps, label) in readings {
        let action = pacer.update(*bps);
        if action != PacerAction::NoChange {
            println!("  {label:25} ({bps:>7} bps): {action:?}");
        }
    }

    // --- Custom config ---
    println!("\n2. Custom config (conservative upgrades, upgrade_streak=5):");
    let cfg = PacerConfig {
        upgrade_streak: 5, // Require 5 consecutive good ticks before upgrading
        ..PacerConfig::default()
    };
    let mut pacer = SubscriberPacer::with_config(cfg);

    // With default config this would upgrade after 3 ticks; now takes 5.
    for i in 1..=6u32 {
        let bps = 400_000;
        let action = pacer.update(bps);
        if action != PacerAction::NoChange {
            println!("  Tick {i}: {action:?}  (upgraded after {i} ticks, not 3)");
        }
    }

    println!("\nDone. See src/bwe/hysteresis.rs for full state-machine details.");
}
