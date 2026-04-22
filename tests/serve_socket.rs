//! Integration test for [`serve_socket`].
//!
//! Verifies that `serve_socket` exits cleanly when the shutdown signal fires,
//! using a real ephemeral UDP socket and an empty [`Registry`].

use std::sync::Arc;

use oxpulse_sfu_kit::udp_loop::{bind, serve_socket};
use oxpulse_sfu_kit::{Registry, SfuConfig, SfuMetrics};

#[tokio::test]
async fn serve_socket_shuts_down_cleanly() {
    let cfg = SfuConfig {
        udp_port: 0,
        bind_address: "127.0.0.1".to_string(),
        ..SfuConfig::default()
    };
    let socket = bind(&cfg).await.expect("bind on ephemeral port");

    let metrics = Arc::new(SfuMetrics::new_default());
    let mut registry = Registry::new(metrics);

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        serve_socket(socket, &mut registry, async {
            let _ = rx.await;
        })
        .await
    });

    tx.send(()).expect("shutdown signal delivered");
    handle
        .await
        .expect("task joined")
        .expect("serve_socket returned Ok");
}
