//! Prometheus metrics for the SFU.
//!
//! One [`SfuMetrics`] per process, wrapped in [`Arc`][std::sync::Arc] and
//! threaded through constructors (no global statics).
//!
//! When the `metrics-prometheus` feature is **off**, all methods on
//! `SfuMetrics` are no-ops and the struct holds no Prometheus handles.
//! This lets `Client` and `Registry` always hold an `Arc<SfuMetrics>` with
//! no conditional compilation at call sites.
//!
//! Submodules:
//! - [`prom`] — full Prometheus implementation (feature `metrics-prometheus`)
//! - [`noop`] — zero-cost stub (feature off)

#[cfg(not(feature = "metrics-prometheus"))]
mod noop;
#[cfg(feature = "metrics-prometheus")]
mod prom;

#[cfg(not(feature = "metrics-prometheus"))]
pub use noop::SfuMetrics;
#[cfg(feature = "metrics-prometheus")]
pub use prom::SfuMetrics;
