//! UDP datagram wrappers over `str0m::net::{Protocol, Transmit}`.

use std::net::SocketAddr;
use std::time::Instant;

/// Transport protocol for an SFU datagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SfuProtocol {
    /// Plain UDP.
    Udp,
    /// UDP over TCP (ICE-TCP).
    Tcp,
    /// SSL over TCP (TURN).
    SslTcp,
    /// TLS over TCP (TURN).
    Tls,
}

impl SfuProtocol {
    #[allow(dead_code)]
    pub(crate) fn from_str0m(p: str0m::net::Protocol) -> Self {
        use str0m::net::Protocol;
        match p {
            Protocol::Udp => Self::Udp,
            Protocol::Tcp => Self::Tcp,
            Protocol::SslTcp => Self::SslTcp,
            Protocol::Tls => Self::Tls,
        }
    }
    #[allow(dead_code)]
    pub(crate) fn to_str0m(self) -> str0m::net::Protocol {
        use str0m::net::Protocol;
        match self {
            Self::Udp => Protocol::Udp,
            Self::Tcp => Protocol::Tcp,
            Self::SslTcp => Protocol::SslTcp,
            Self::Tls => Protocol::Tls,
        }
    }
}

/// A datagram received from the network, ready to feed into an `SfuRtc` via
/// [`Client::handle_input`][crate::Client::handle_input] (migration lands in Task 7).
#[derive(Debug)]
pub struct IncomingDatagram {
    /// Wall-clock time the datagram was received.
    pub received_at: Instant,
    /// Transport protocol.
    pub proto: SfuProtocol,
    /// Remote address.
    pub source: SocketAddr,
    /// Local bound address.
    pub destination: SocketAddr,
    /// Raw wire bytes.
    pub contents: Vec<u8>,
}

/// A datagram the SFU wants to send.
#[derive(Debug)]
pub struct OutgoingDatagram {
    /// Transport protocol.
    pub proto: SfuProtocol,
    /// Source (local) address.
    pub source: SocketAddr,
    /// Destination (remote) address.
    pub destination: SocketAddr,
    /// Wire bytes.
    pub contents: Vec<u8>,
}

impl OutgoingDatagram {
    #[allow(dead_code)]
    pub(crate) fn from_transmit(t: str0m::net::Transmit) -> Self {
        Self {
            proto: SfuProtocol::from_str0m(t.proto),
            source: t.source,
            destination: t.destination,
            contents: t.contents.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_roundtrip() {
        use str0m::net::Protocol;
        for p in [
            Protocol::Udp,
            Protocol::Tcp,
            Protocol::SslTcp,
            Protocol::Tls,
        ] {
            let wrapped = SfuProtocol::from_str0m(p);
            assert_eq!(wrapped.to_str0m(), p);
        }
    }
}
