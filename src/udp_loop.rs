//! Async UDP socket loop.
//!
//! Binds a UDP port, demuxes incoming datagrams through the [`Registry`],
//! flushes each client's outbound queue back to the socket, and honors a
//! shutdown future so the caller can stop the loop cleanly.
//!
//! Client registration (SDP offer/answer, ICE) is the signaling layer's job —
//! bring your own WebSocket or HTTP server. Insert clients via
//! [`Registry::insert`][crate::Registry::insert].

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use tokio::net::UdpSocket;
use tokio::time::MissedTickBehavior;

use crate::config::SfuConfig;
use crate::metrics::SfuMetrics;
use crate::registry::Registry;

/// Tick interval for the dominant-speaker detector (300 ms, matches mediasoup).
/// Always defined so the select! branch compiles regardless of the feature flag.
const ASO_TICK_MS: u64 = 300;

/// Maximum UDP payload the SFU expects to receive.
///
/// 2048 bytes covers STUN / DTLS / SRTP with a comfortable margin under the
/// typical 1500-byte Ethernet MTU. Matches str0m's `chat.rs` value.
const RECV_BUFFER_BYTES: usize = 2048;

/// Upper bound on how long the receive branch is allowed to park.
///
/// Keeps the str0m tick loop (which wants ~100 ms granularity) from starving
/// when no datagrams arrive.
const MAX_SLEEP: Duration = Duration::from_millis(100);

/// Run the SFU UDP loop until `shutdown` resolves.
///
/// Binds `config.udp_port` on `config.bind_address`, then drives the receive
/// loop. If you need to know the resolved port (e.g. when `udp_port = 0`),
/// call [`bind`] and [`serve`] separately.
pub async fn run_udp_loop<F>(config: SfuConfig, shutdown: F) -> anyhow::Result<()>
where
    F: Future<Output = ()>,
{
    let metrics = Arc::new(SfuMetrics::new_default());
    let socket = bind(&config).await?;
    serve(socket, metrics, shutdown).await
}

/// Bind the UDP socket per `config`.
///
/// Exposed so callers can observe the resolved `local_addr` when `udp_port = 0`.
pub async fn bind(config: &SfuConfig) -> anyhow::Result<UdpSocket> {
    let addr = format!("{}:{}", config.bind_address, config.udp_port);
    let socket = UdpSocket::bind(&addr)
        .await
        .with_context(|| format!("failed to bind UDP socket at {addr}"))?;
    let local = socket.local_addr().context("failed to read local_addr")?;
    tracing::info!(%local, "SFU starting — UDP listener ready");
    Ok(socket)
}

/// Drive the receive loop on an already-bound socket.
///
/// Constructs a [`Registry`] internally. For multi-room deployments where the
/// caller manages the registry lifecycle, use [`serve_socket`] instead.
///
/// Returns once `shutdown` resolves or a fatal socket error occurs.
pub async fn serve<F>(
    socket: UdpSocket,
    metrics: Arc<SfuMetrics>,
    shutdown: F,
) -> anyhow::Result<()>
where
    F: Future<Output = ()>,
{
    let mut registry = Registry::new(metrics);
    serve_socket(socket, &mut registry, shutdown).await
}

/// Serve a single UDP socket against a caller-owned [`Registry`].
///
/// Useful for multi-room deployments where the caller controls socket
/// lifecycle and peer registration independently of the receive loop.
///
/// Returns once `shutdown` resolves or a fatal socket error occurs.
pub async fn serve_socket<F>(
    socket: UdpSocket,
    registry: &mut Registry,
    shutdown: F,
) -> anyhow::Result<()>
where
    F: Future<Output = ()>,
{
    let local = socket.local_addr().context("failed to read local_addr")?;
    let mut buf = vec![0u8; RECV_BUFFER_BYTES];
    let mut aso_interval = tokio::time::interval(Duration::from_millis(ASO_TICK_MS));
    aso_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    tokio::pin!(shutdown);

    loop {
        registry.reap_dead();

        let deadline = registry.poll_all(Instant::now());
        registry.fanout_pending();
        registry.emit_publisher_layer_hints();
        flush_transmits(&socket, registry).await;

        let sleep = deadline
            .saturating_duration_since(Instant::now())
            .max(Duration::from_millis(1))
            .min(MAX_SLEEP);

        tokio::select! {
            () = &mut shutdown => {
                tracing::info!("SFU shutting down — UDP loop stopping");
                return Ok(());
            }
            _ = tokio::time::sleep(sleep) => {
                registry.tick(Instant::now());
            }
            _ = aso_interval.tick() => {
                // active-speaker feature: advance the dominant-speaker detector.
                #[cfg(feature = "active-speaker")]
                registry.tick_active_speaker(Instant::now());
                // Update per-peer speaker score Prometheus gauges.
                #[cfg(all(feature = "active-speaker", feature = "metrics-prometheus"))]
                registry.tick_speaker_scores();
                // Without the feature, just tick str0m's session clock.
                #[cfg(not(feature = "active-speaker"))]
                registry.tick(Instant::now());
            }
            recv = socket.recv_from(&mut buf) => {
                match recv {
                    Ok((n, src)) => {
                        registry.handle_incoming(src, local, &buf[..n]);
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "udp recv_from failed");
                    }
                }
            }
        }
    }
}

async fn flush_transmits(socket: &UdpSocket, registry: &mut Registry) {
    let mut pending = Vec::new();
    registry.drain_transmits(|t| pending.push(t));
    for t in pending {
        if let Err(e) = socket.send_to(&t.contents, t.destination).await {
            tracing::warn!(
                dest = %t.destination,
                error = %e,
                "udp send_to failed",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bind_uses_ephemeral_port_when_zero() {
        let cfg = SfuConfig {
            udp_port: 0,
            ..SfuConfig::default()
        };
        let socket = bind(&cfg).await.expect("bind succeeds on 0.0.0.0:0");
        let got = socket.local_addr().expect("local_addr");
        assert_ne!(got.port(), 0, "kernel must assign a real ephemeral port");
    }

    #[tokio::test]
    async fn serve_shuts_down_cleanly() {
        let cfg = SfuConfig {
            udp_port: 0,
            bind_address: "127.0.0.1".to_string(),
            ..SfuConfig::default()
        };
        let socket = bind(&cfg).await.expect("bind");
        let metrics = Arc::new(SfuMetrics::new_default());
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(serve(socket, metrics, async {
            let _ = rx.await;
        }));
        tx.send(()).unwrap();
        handle.await.unwrap().unwrap();
    }
}
