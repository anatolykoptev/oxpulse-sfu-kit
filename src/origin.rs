//! Publisher origin — local client or upstream SFU relay connection.

/// Whether this client represents a direct peer or an upstream SFU relay.
///
/// `Local` is the default — a direct WebRTC peer connection. `RelayFromSfu`
/// is set by the application after ICE/DTLS completes on the edge-to-edge
/// relay connection; the string is an opaque upstream edge identifier
/// (typically URL or region ID) used for logging and diagnostics.
///
/// # Call order
///
/// Call [`Client::set_origin`][crate::Client::set_origin] **before**
/// [`Registry::insert`][crate::Registry::insert]. The insert path reads
/// `is_relay()` to decide whether to register the peer with the
/// dominant-speaker detector.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ClientOrigin {
    /// A normal WebRTC peer connected directly to this SFU.
    #[default]
    Local,
    /// A relay connection from another SFU edge.
    ///
    /// The string is typically the upstream edge URL or region identifier.
    /// It has no semantic effect inside the kit.
    RelayFromSfu(String),
}
