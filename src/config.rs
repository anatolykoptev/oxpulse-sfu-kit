//! SFU runtime configuration.
//!
//! [`SfuConfig`] is environment-driven with sensible defaults. Construct via
//! [`SfuConfig::default`] for tests or [`SfuConfig::from_env`] for production.

/// Runtime configuration for the SFU.
///
/// All fields have sensible defaults. Override via environment variables or
/// struct literals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SfuConfig {
    /// UDP port the SFU listens on for WebRTC media (DTLS/SRTP/STUN
    /// multiplexed over a single socket per the str0m `chat.rs` pattern).
    ///
    /// Env: `SFU_UDP_PORT`. Default: `3478`.
    pub udp_port: u16,
    /// HTTP port for Prometheus `/metrics` (only used by the example binary
    /// and `spawn_metrics_server` if you opt in to `metrics-prometheus`).
    ///
    /// Env: `SFU_METRICS_PORT`. Default: `9317`.
    pub metrics_port: u16,
    /// Bind address for the UDP socket.
    ///
    /// Env: `SFU_BIND_ADDRESS`. Default: `"0.0.0.0"`.
    pub bind_address: String,
    /// `RUST_LOG`-style directive forwarded to `tracing_subscriber` in the
    /// example binary. The library itself does not initialize logging.
    ///
    /// Env: `RUST_LOG`. Default: `"info"`.
    pub log_level: String,
}

impl Default for SfuConfig {
    fn default() -> Self {
        Self {
            udp_port: 3478,
            metrics_port: 9317,
            bind_address: "0.0.0.0".to_string(),
            log_level: "info".to_string(),
        }
    }
}

impl SfuConfig {
    /// Build a config from environment variables, falling back to defaults.
    ///
    /// # Panics
    ///
    /// Panics at startup if `SFU_UDP_PORT` or `SFU_METRICS_PORT` are set to
    /// non-numeric values.
    pub fn from_env() -> Self {
        let defaults = Self::default();
        Self {
            udp_port: env("SFU_UDP_PORT", &defaults.udp_port.to_string())
                .parse()
                .expect("SFU_UDP_PORT must be a number"),
            metrics_port: env("SFU_METRICS_PORT", &defaults.metrics_port.to_string())
                .parse()
                .expect("SFU_METRICS_PORT must be a number"),
            bind_address: env("SFU_BIND_ADDRESS", &defaults.bind_address),
            log_level: env("RUST_LOG", &defaults.log_level),
        }
    }
}

fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_sensible() {
        let cfg = SfuConfig::default();
        assert_eq!(cfg.bind_address, "0.0.0.0");
        assert_eq!(cfg.udp_port, 3478);
        assert_ne!(cfg.udp_port, cfg.metrics_port);
    }
}
