//! Basic single-node SFU example.
//!
//! Binds a UDP port for WebRTC media and a TCP port for Prometheus metrics.
//! Signaling (SDP offer/answer, ICE candidate exchange) is out of scope for
//! this example — wire up your own WebSocket server and call
//! `registry.insert(client)` once ICE/DTLS is complete.
//!
//! Run with:
//! ```not_rust
//! cargo run --example basic-sfu --features active-speaker,metrics-prometheus
//! ```
//!
//! Environment variables:
//! - `SFU_UDP_PORT` (default 3478)
//! - `SFU_METRICS_PORT` (default 9317)
//! - `SFU_BIND_ADDRESS` (default 0.0.0.0)
//! - `RUST_LOG` (default info)

use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use oxpulse_sfu_kit::metrics::SfuMetrics;
use oxpulse_sfu_kit::{udp_loop, SfuConfig};
use tokio::signal;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = SfuConfig::from_env();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level)),
        )
        .init();

    let metrics = Arc::new(SfuMetrics::new_default());

    // Spawn the Prometheus HTTP server.
    let metrics_addr = format!("{}:{}", config.bind_address, config.metrics_port);
    let metrics_handle = spawn_metrics_server(metrics_addr, metrics.clone())?;

    // Shutdown future: resolves on SIGINT or SIGTERM.
    let shutdown = async move {
        #[cfg(unix)]
        {
            let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("failed to install SIGTERM handler");
            tokio::select! {
                res = signal::ctrl_c() => match res {
                    Ok(()) => tracing::info!("received SIGINT"),
                    Err(e) => tracing::error!(error = %e, "ctrl_c handler failed"),
                },
                _ = sigterm.recv() => tracing::info!("received SIGTERM"),
            }
        }
        #[cfg(not(unix))]
        match signal::ctrl_c().await {
            Ok(()) => tracing::info!("received SIGINT"),
            Err(e) => tracing::error!(error = %e, "ctrl_c handler failed"),
        }
    };

    let socket = udp_loop::bind(&config).await?;
    let result = udp_loop::serve(socket, metrics, shutdown).await;

    metrics_handle.abort();
    result
}

fn spawn_metrics_server(
    bind_addr: String,
    metrics: Arc<SfuMetrics>,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    use std::net::TcpListener;

    let listener = TcpListener::bind(&bind_addr)
        .map_err(|e| anyhow::anyhow!("bind metrics at {bind_addr}: {e}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| anyhow::anyhow!("set_nonblocking: {e}"))?;
    let tok_listener = tokio::net::TcpListener::from_std(listener)
        .map_err(|e| anyhow::anyhow!("convert TcpListener: {e}"))?;

    tracing::info!(%bind_addr, "SFU metrics server ready");

    let handle = tokio::spawn(async move {
        use axum::response::IntoResponse;
        let app = Router::new().route(
            "/metrics",
            get(move || {
                let m = metrics.clone();
                async move {
                    match m.encode_text() {
                        Ok(body) => (
                            axum::http::StatusCode::OK,
                            [(
                                axum::http::header::CONTENT_TYPE,
                                "text/plain; version=0.0.4",
                            )],
                            body,
                        )
                            .into_response(),
                        Err(e) => {
                            tracing::warn!(error = %e, "metrics encode failed");
                            (
                                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                                [(axum::http::header::CONTENT_TYPE, "text/plain")],
                                "encode failed".to_string(),
                            )
                                .into_response()
                        }
                    }
                }
            }),
        );
        if let Err(e) = axum::serve(tok_listener, app).await {
            tracing::warn!(error = %e, "metrics server exited");
        }
    });

    Ok(handle)
}
