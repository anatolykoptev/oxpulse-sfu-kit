//! SFrame (RFC 9605) key-epoch helper.
//!
//! The SFU does not encrypt or decrypt payloads — SFrame encryption is
//! frame-level and end-to-end (publisher ↔ subscriber). The SFrame ciphertext,
//! including its in-payload header (which carries the KID), is forwarded opaquely
//! as part of the RTP payload. The SFU does NOT parse or re-attach any key-id RTP
//! *header extension*; if your clients use one, plumb the key epoch out-of-band
//! (e.g. a dedicated DataChannel or your signalling layer).
//!
//! [`KeyEpoch`] is a convenience newtype for that app-side epoch bookkeeping — it
//! is not read from or written to the RTP forwarding path by this crate.

/// An SFrame key-epoch (KID) value, for app-side epoch bookkeeping.
///
/// Maps to the `KID` (key identifier) field in SFrame (RFC 9605 §4.2).
/// Increment on each group key rotation; receivers use it to select the correct
/// decryption key. This crate does not read or write this value on the RTP
/// forwarding path — the application manages and distributes it out-of-band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyEpoch(pub u64);

impl KeyEpoch {
    /// Create from a raw `u64` KID value.
    pub fn new(kid: u64) -> Self {
        Self(kid)
    }

    /// Raw KID value.
    #[must_use]
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_epoch_roundtrip() {
        let k = KeyEpoch::new(42);
        assert_eq!(k.as_u64(), 42);
    }

    #[test]
    fn key_epoch_zero() {
        let k = KeyEpoch::new(0);
        assert_eq!(k.as_u64(), 0);
    }
}
