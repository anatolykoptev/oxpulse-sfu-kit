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
//!
//! # Trust
//!
//! The forwarded header-extension KID is a **non-authoritative hint**. The SFU
//! is untrusted and never validates it; a malicious or buggy publisher can set
//! any value, including one that disagrees with the KID in the opaque
//! in-payload SFrame header. Receivers MUST still select the key from the
//! in-payload header and let the AEAD fail closed on a mismatch — a wrong hint
//! costs at most a dropped frame, never confidentiality.

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

/// On-the-wire width of an RFC 9605 §6.1 (v1) KID: a fixed 4-byte, big-endian
/// 32-bit value. [`KeyEpochSerializer`] uses this width; a consumer writing its
/// own serializer for a non-standard layout can reference it.
pub const SFRAME_KID_WIDTH: usize = 4;

/// Reference [`ExtensionSerializer`] for the SFrame key-epoch (KID) RTP header
/// extension.
///
/// # Wire format
///
/// A **fixed 4-byte, big-endian 32-bit KID**, matching RFC 9605 §6.1 (the v1
/// KID width) and the `sframe-ratchet` browser client. This is the
/// interoperable default; if your clients encode the KID differently, implement
/// your own serializer that parses into / writes from a [`KeyEpoch`] user value
/// — the fanout path keys on the `KeyEpoch` **type**, not these bytes.
///
/// [`KeyEpoch`] holds a `u64` for generality, but the RFC v1 KID is 32-bit: a
/// value exceeding `u32::MAX` cannot be represented in the 4-byte form and is
/// **not written** (str0m then omits the extension), rather than silently
/// truncated. Supply your own serializer for wider KIDs.
///
/// The serializer applies to **both** audio and video media (SFrame protects
/// either). Its 4-byte value never *requires* the two-byte extension form
/// (str0m may still pick that form globally if another registered extension
/// needs it — a full `u8` length encodes 4 bytes fine).
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyEpochSerializer;

impl ExtensionSerializer for KeyEpochSerializer {
    fn write_to(&self, buf: &mut [u8], ev: &ExtensionValues) -> usize {
        let Some(ke) = ev.user_values.get::<KeyEpoch>() else {
            return 0;
        };
        // RFC 9605 §6.1 v1 KID is 32-bit; a wider value is unrepresentable in
        // the 4-byte form — omit rather than truncate to a wrong KID.
        let Ok(kid32) = u32::try_from(ke.as_u64()) else {
            return 0;
        };
        if buf.len() < SFRAME_KID_WIDTH {
            return 0;
        }
        buf[..SFRAME_KID_WIDTH].copy_from_slice(&kid32.to_be_bytes());
        SFRAME_KID_WIDTH
    }

    fn parse_value(&self, buf: &[u8], ev: &mut ExtensionValues) -> bool {
        // Fixed-width: accept exactly the 4-byte RFC v1 KID, nothing else.
        let Ok(bytes) = <[u8; SFRAME_KID_WIDTH]>::try_from(buf) else {
            return false;
        };
        ev.user_values
            .set(KeyEpoch::new(u64::from(u32::from_be_bytes(bytes))));
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

    /// The reference serializer round-trips a 32-bit KID through the fixed
    /// 4-byte RFC 9605 §6.1 wire format: what a subscriber's str0m parses out
    /// must equal what the SFU's str0m serialized in. Exercises the exact
    /// `write_to` / `parse_value` calls str0m makes on fanout write and ingest.
    #[test]
    fn key_epoch_serializer_wire_roundtrip() {
        let ser = KeyEpochSerializer;
        for kid in [0u64, 1, 7, 255, 256, 65_535, u64::from(u32::MAX)] {
            let mut out = ExtensionValues::default();
            out.user_values.set(KeyEpoch::new(kid));

            let mut buf = [0u8; 16];
            let n = ser.write_to(&mut buf, &out);
            assert_eq!(n, SFRAME_KID_WIDTH, "kid={kid} must write a fixed 4 bytes");

            let mut parsed = ExtensionValues::default();
            assert!(ser.parse_value(&buf[..n], &mut parsed), "kid={kid} parse");
            assert_eq!(
                parsed.user_values.get::<KeyEpoch>().copied(),
                Some(KeyEpoch::new(kid)),
                "kid={kid} did not round-trip on the wire"
            );
        }
    }

    /// A KID exceeding the RFC v1 32-bit width is unrepresentable in the 4-byte
    /// form: the serializer omits it (writes 0) rather than truncating to a
    /// wrong KID. Consumers with wider KIDs must supply their own serializer.
    #[test]
    fn key_epoch_serializer_omits_kid_above_u32() {
        let ser = KeyEpochSerializer;
        let mut out = ExtensionValues::default();
        out.user_values.set(KeyEpoch::new(u64::from(u32::MAX) + 1));
        let mut buf = [0u8; 16];
        assert_eq!(ser.write_to(&mut buf, &out), 0);
    }

    /// A destination buffer smaller than the 4-byte KID width is refused rather
    /// than writing a partial/truncated KID.
    #[test]
    fn key_epoch_serializer_refuses_short_buffer() {
        let ser = KeyEpochSerializer;
        let mut out = ExtensionValues::default();
        out.user_values.set(KeyEpoch::new(7));
        let mut buf = [0u8; SFRAME_KID_WIDTH - 1];
        assert_eq!(ser.write_to(&mut buf, &out), 0);
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

    /// Fixed-width parse: only an exactly-4-byte buffer is accepted; a short,
    /// long, or empty buffer is rejected rather than mis-parsed.
    #[test]
    fn key_epoch_serializer_rejects_wrong_width() {
        let ser = KeyEpochSerializer;
        let mut parsed = ExtensionValues::default();
        assert!(!ser.parse_value(&[], &mut parsed));
        assert!(!ser.parse_value(&[0u8; 3], &mut parsed));
        assert!(!ser.parse_value(&[0u8; 5], &mut parsed));
        assert!(!ser.parse_value(&[0u8; 9], &mut parsed));
        assert!(ser.parse_value(&[0u8; SFRAME_KID_WIDTH], &mut parsed));
    }
}
