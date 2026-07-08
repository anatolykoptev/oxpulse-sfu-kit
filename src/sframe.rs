//! SFrame (RFC 9605) key-epoch (KID) RTP header-extension forwarding.
//!
//! The SFU does not encrypt or decrypt payloads — SFrame encryption is
//! frame-level and end-to-end (publisher ↔ subscriber). The SFrame ciphertext,
//! including its in-payload header, is forwarded opaquely as part of the RTP
//! payload.
//!
//! Some deployments additionally signal the current key epoch (KID) in a
//! dedicated **RTP header extension** so a subscriber can pick the right
//! decryption key without waiting for the in-payload header. This crate
//! forwards that header extension on the media path: [`KeyEpoch`] captured off
//! an inbound packet is re-attached to every fanned-out packet
//! (`SfuMediaPayload::key_epoch` → `Writer::user_extension_value`).
//!
//! # Enabling it
//!
//! str0m only parses/serializes a header extension whose URI is registered on
//! the [`Rtc`][str0m::Rtc] and negotiated in SDP — exactly like `audio-level`
//! or `mid`. Because the SFrame-KID extension has no standard URI, the URI is
//! **yours to choose** (it must match what your clients put in their SDP
//! `a=extmap`). Register [`sframe_key_id_extension`] on the raw str0m config:
//!
//! ```no_run
//! use oxpulse_sfu_kit::{raw, sframe, SfuRtc};
//!
//! // Pick the URI your clients negotiate for the SFrame KID extension.
//! const SFRAME_KID_URI: &str = "urn:example:rtp-hdrext:sframe-kid";
//!
//! let cfg = raw::rtc_config()
//!     .set_extension(8, sframe::sframe_key_id_extension(SFRAME_KID_URI));
//! let rtc = SfuRtc::from_raw(cfg.build(std::time::Instant::now()));
//! ```
//!
//! Register it on **every** peer's `Rtc` (the publisher side parses inbound,
//! the subscriber side serializes outbound). Once registered + negotiated, the
//! SFU forwards the KID transparently and never inspects the encrypted payload.
//!
//! [`sframe_key_id_extension`] uses [`KeyEpochSerializer`], a reference wire
//! format (see its docs). If your clients encode the KID differently, supply
//! your own [`ExtensionSerializer`][str0m::rtp::ExtensionSerializer] that parses
//! into / writes from a [`KeyEpoch`] user value — the fanout path keys on the
//! `KeyEpoch` **type**, not on the wire bytes.
//!
//! Key distribution (e.g. MLS, RFC 9420) remains your signalling layer's job.

use str0m::rtp::{Extension, ExtensionSerializer, ExtensionValues};

/// An SFrame key-epoch (KID) value.
///
/// Maps to the `KID` (key identifier) field in SFrame (RFC 9605 §4.2).
/// Increment on each group key rotation; receivers use it to select the correct
/// decryption key. When an SFrame-KID RTP header extension is registered (see
/// the [module docs][crate::sframe]), the SFU parses this value off inbound
/// packets and re-attaches it to fanned-out packets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyEpoch(pub u64);

impl KeyEpoch {
    /// Create from a raw `u64` KID value.
    #[must_use]
    pub fn new(kid: u64) -> Self {
        Self(kid)
    }

    /// Raw KID value.
    #[must_use]
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// Reference [`ExtensionSerializer`] for the SFrame key-epoch (KID) RTP header
/// extension.
///
/// # Wire format
///
/// The KID is encoded as its minimal-length big-endian byte sequence (1–8
/// bytes; a KID of `0` is a single `0x00` byte). This mirrors how small
/// identifiers are carried compactly in RTP header extensions. If your clients
/// use a different layout, implement your own serializer that parses into /
/// writes from a [`KeyEpoch`] user value.
///
/// The serializer applies to **both** audio and video media (SFrame protects
/// either), and never uses the two-byte extension form (the value is ≤ 8 bytes).
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyEpochSerializer;

impl ExtensionSerializer for KeyEpochSerializer {
    fn write_to(&self, buf: &mut [u8], ev: &ExtensionValues) -> usize {
        let Some(ke) = ev.user_values.get::<KeyEpoch>() else {
            return 0;
        };
        let bytes = ke.as_u64().to_be_bytes();
        // Minimal big-endian: strip leading zero bytes, but always keep ≥ 1.
        let start = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len() - 1);
        let slice = &bytes[start..];
        if slice.len() > buf.len() {
            return 0;
        }
        buf[..slice.len()].copy_from_slice(slice);
        slice.len()
    }

    fn parse_value(&self, buf: &[u8], ev: &mut ExtensionValues) -> bool {
        if buf.is_empty() || buf.len() > 8 {
            return false;
        }
        let mut v = 0u64;
        for &b in buf {
            v = (v << 8) | u64::from(b);
        }
        ev.user_values.set(KeyEpoch::new(v));
        true
    }

    fn is_video(&self) -> bool {
        true
    }

    fn is_audio(&self) -> bool {
        true
    }
}

/// Build a str0m [`Extension`] for the SFrame key-epoch (KID) header extension,
/// bound to the given `uri`.
///
/// Register the result on the raw str0m config so str0m parses the extension on
/// ingest and serializes it on egress; see the [module docs][crate::sframe].
/// The `uri` must match the `a=extmap` URI your clients negotiate.
#[must_use]
pub fn sframe_key_id_extension(uri: &str) -> Extension {
    Extension::with_serializer(uri, KeyEpochSerializer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_epoch_newtype_roundtrip() {
        let k = KeyEpoch::new(42);
        assert_eq!(k.as_u64(), 42);
        assert_eq!(KeyEpoch::new(0).as_u64(), 0);
    }

    /// The reference serializer round-trips a KID through the RTP header
    /// extension wire format: what a subscriber's str0m parses out must equal
    /// what the SFU's str0m serialized in. Exercises the exact `write_to` /
    /// `parse_value` calls str0m makes on the fanout write and on ingest.
    #[test]
    fn key_epoch_serializer_wire_roundtrip() {
        let ser = KeyEpochSerializer;
        for kid in [0u64, 1, 7, 255, 256, 65_535, 1 << 40, u64::MAX] {
            let mut out = ExtensionValues::default();
            out.user_values.set(KeyEpoch::new(kid));

            let mut buf = [0u8; 16];
            let n = ser.write_to(&mut buf, &out);
            assert!((1..=8).contains(&n), "kid={kid} wrote {n} bytes");

            let mut parsed = ExtensionValues::default();
            assert!(ser.parse_value(&buf[..n], &mut parsed), "kid={kid} parse");
            assert_eq!(
                parsed.user_values.get::<KeyEpoch>().copied(),
                Some(KeyEpoch::new(kid)),
                "kid={kid} did not round-trip on the wire"
            );
        }
    }

    /// With no `KeyEpoch` user value present, the serializer writes nothing so
    /// str0m omits the extension entirely (no spurious zero-length extension).
    #[test]
    fn key_epoch_serializer_absent_writes_nothing() {
        let ser = KeyEpochSerializer;
        let ev = ExtensionValues::default();
        let mut buf = [0u8; 16];
        assert_eq!(ser.write_to(&mut buf, &ev), 0);
    }

    /// Rejects an over-long buffer rather than silently truncating the KID.
    #[test]
    fn key_epoch_serializer_rejects_oversized_buf() {
        let ser = KeyEpochSerializer;
        let mut parsed = ExtensionValues::default();
        assert!(!ser.parse_value(&[0u8; 9], &mut parsed));
        assert!(!ser.parse_value(&[], &mut parsed));
    }
}
