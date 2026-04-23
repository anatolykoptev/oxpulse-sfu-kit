//! SFrame (RFC 9605) key-epoch forwarding seam.
//!
//! The SFU does not encrypt or decrypt payloads — SFrame encryption is
//! frame-level and end-to-end (publisher ↔ subscriber). This module provides
//! the [`KeyEpoch`] newtype for forwarding the key-epoch RTP header extension
//! so application code can route key distribution independently of the SFU
//! forwarding path.

/// The key-epoch value carried in the SFrame RTP header extension.
///
/// Maps to the `KID` (key identifier) field in SFrame (RFC 9605 §4.2).
/// Increment on each group key rotation. Receivers use this to select the
/// correct decryption key.
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
