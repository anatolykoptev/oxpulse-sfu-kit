//! Prometheus metrics integration tests (requires `metrics-prometheus` + `test-utils`).
//!
//! Verifies that connect/forward counters appear in the scraped output and that
//! the metrics server shuts down cleanly when aborted.

use std::sync::Arc;
use std::time::Duration;

use str0m::media::MediaKind;
use oxpulse_sfu_kit::client::layer;
use oxpulse_sfu_kit::client::test_seed::{make_media_data, new_client, seed_track_in};
use oxpulse_sfu_kit::metrics::SfuMetrics;
use oxpulse_sfu_kit::{udp_loop, ClientId, Propagated, Registry, SfuConfig};
use tokio::net::UdpSocket;
use tokio::sync::oneshot;
use tokio::time::timeout;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Spawn a metrics HTTP server on 127.0.0.1:0 and return (port, handle, metrics).
fn bind_metrics_server() -> (u16, tokio::task::JoinHandle<()>, Arc<SfuMetrics>) {
    use axum::routing::get;
    use axum::Router;
    use std::net::TcpListener;

    let probe = TcpListener::bind("127.0.0.1:0").expect("probe bind");
    let port = probe.local_addr().expect("local_addr").port();
    drop(probe);

    let metrics = Arc::new(SfuMetrics::new_default());
    let m = metrics.clone();

    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).expect("bind metrics");
    listener.set_nonblocking(true).expect("set_nonblocking");
    let tok_listener =
        tokio::net::TcpListener::from_std(listener).expect("convert to tokio listener");

    let handle = tokio::spawn(async move {
        use axum::response::IntoResponse;
        let app = Router::new().route(
            "/metrics",
            get(move || {
                let m = m.clone();
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

    (port, handle, metrics)
}

async fn scrape(port: u16) -> reqwest::Result<String> {
    reqwest::get(format!("http://127.0.0.1:{port}/metrics"))
        .await?
        .text()
        .await
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn udp_loop_serves_with_registry_and_shuts_down() {
    let cfg = SfuConfig {
        udp_port: 0,
        bind_address: "127.0.0.1".to_string(),
        ..SfuConfig::default()
    };

    let server_sock = udp_loop::bind(&cfg).await.expect("bind");
    let local = server_sock.local_addr().expect("local_addr");
    let metrics = Arc::new(SfuMetrics::new_default());

    let (tx, rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        udp_loop::serve(server_sock, metrics, async {
            let _ = rx.await;
        })
        .await
    });

    let client = UdpSocket::bind("127.0.0.1:0").await.expect("client bind");
    client
        .send_to(b"stun-probe-no-match", local)
        .await
        .expect("send");

    tokio::time::sleep(Duration::from_millis(50)).await;
    tx.send(()).expect("shutdown");

    let out = timeout(Duration::from_secs(2), handle)
        .await
        .expect("loop terminates")
        .expect("task did not panic");
    out.expect("serve returned Ok");
}

#[tokio::test]
async fn metrics_track_client_and_packet_counts() {
    let (port, _handle, metrics) = bind_metrics_server();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut registry = Registry::new(metrics.clone());

    let mut a = new_client(ClientId(100));
    let _track = seed_track_in(&mut a, 1, MediaKind::Video);
    registry.insert(a);
    let b = new_client(ClientId(101));
    registry.insert(b);

    // Forward one video packet (RID=q — matches B's default LOW layer).
    let prop = Propagated::MediaData(ClientId(100), make_media_data(1, Some(layer::LOW)));
    registry.fanout_for_tests(&prop);

    let body = timeout(Duration::from_secs(3), scrape(port))
        .await
        .expect("scrape timeout")
        .expect("scrape ok");

    assert!(
        body.contains("sfu_client_connect_total"),
        "connect counter:\n{body}"
    );
    assert!(
        body.contains("sfu_active_participants"),
        "participants gauge:\n{body}"
    );
    assert!(
        body.contains(r#"sfu_forwarded_packets_total{kind="video"}"#),
        "forwarded video counter:\n{body}",
    );

    for line in body.lines() {
        if line.starts_with("sfu_client_connect_total ") {
            let v: f64 = line.split_whitespace().nth(1).unwrap().parse().unwrap();
            assert!(v >= 2.0, "client_connect_total >= 2, got {v}");
        }
        if line.starts_with("sfu_active_participants ") {
            let v: f64 = line.split_whitespace().nth(1).unwrap().parse().unwrap();
            assert_eq!(v, 2.0, "active_participants = 2, got {v}");
        }
    }
}

#[tokio::test]
async fn shutdown_stops_metrics_server() {
    let (port, handle, _metrics) = bind_metrics_server();
    tokio::time::sleep(Duration::from_millis(50)).await;

    scrape(port).await.expect("scrape before shutdown");

    handle.abort();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let result = timeout(Duration::from_secs(2), scrape(port)).await;
    match result {
        Ok(Err(_)) => {} // connection refused — expected
        Ok(Ok(_)) => panic!("metrics server still responding after shutdown"),
        Err(_) => panic!("scrape timed out instead of failing fast"),
    }
}
