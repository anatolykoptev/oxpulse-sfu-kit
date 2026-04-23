//! RFC 9626 Video Frame Marking RTP header extension parser.
//!
//! Parses the 1-byte short form:
//! ```text
//!  0 1 2 3 4 5 6 7
//! +-+-+-+-+-+-+-+-+
//! |S|E|I|D|B| TID |
//! +-+-+-+-+-+-+-+-+
//! ```
//! S = start of frame, E = end, I = independent, D = discardable,
//! B = base-layer sync, TID = temporal layer ID (0–7).
//!
//! Reference: RFC 9626 §4, `urn:ietf:params:rtp-hdrext:framemarking`.

/// Parsed Video Frame Marking header extension (RFC 9626 short form).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameMarkingInfo {
    /// First packet of a frame.
    pub start_of_frame: bool,
    /// Last packet of a frame.
    pub end_of_frame: bool,
    /// Frame can be decoded without prior frames (IDR / key frame).
    pub independent: bool,
    /// Frame can be discarded without visible artifacts.
    pub discardable: bool,
    /// Base layer sync point.
    pub base_layer_sync: bool,
    /// Temporal layer ID (0 = base, 1 = half-rate, 2 = quarter-rate, …).
    pub temporal_id: u8,
}

/// Parse a raw Video Frame Marking RTP header extension payload.
///
/// Only the first byte (short form) is parsed. Extended forms (3-byte)
/// are handled by reading byte 0 only; the additional `LID`/`TL0PICIDX`
/// bytes are intentionally ignored for SFU forwarding purposes.
///
/// Returns `None` if `bytes` is empty.
pub fn parse(bytes: &[u8]) -> Option<FrameMarkingInfo> {
    let b = *bytes.first()?;
    Some(FrameMarkingInfo {
        start_of_frame:  (b >> 7) & 1 == 1,
        end_of_frame:    (b >> 6) & 1 == 1,
        independent:     (b >> 5) & 1 == 1,
        discardable:     (b >> 4) & 1 == 1,
        base_layer_sync: (b >> 3) & 1 == 1,
        temporal_id:      b & 0x07,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_byte(s: bool, e: bool, i: bool, d: bool, b: bool, tid: u8) -> u8 {
        ((s as u8) << 7)
            | ((e as u8) << 6)
            | ((i as u8) << 5)
            | ((d as u8) << 4)
            | ((b as u8) << 3)
            | (tid & 0x07)
    }

    #[test]
    fn empty_returns_none() {
        assert!(parse(&[]).is_none());
    }

    #[test]
    fn base_layer_keyframe() {
        // S=1 E=1 I=1 D=0 B=1 TID=0
        let byte = make_byte(true, true, true, false, true, 0);
        let info = parse(&[byte]).unwrap();
        assert!(info.start_of_frame);
        assert!(info.end_of_frame);
        assert!(info.independent);
        assert!(!info.discardable);
        assert!(info.base_layer_sync);
        assert_eq!(info.temporal_id, 0);
    }

    #[test]
    fn temporal_layer_2_discardable() {
        // S=0 E=1 I=0 D=1 B=0 TID=2
        let byte = make_byte(false, true, false, true, false, 2);
        let info = parse(&[byte]).unwrap();
        assert!(!info.start_of_frame);
        assert!(info.end_of_frame);
        assert!(!info.independent);
        assert!(info.discardable);
        assert!(!info.base_layer_sync);
        assert_eq!(info.temporal_id, 2);
    }

    #[test]
    fn max_temporal_id_7() {
        let byte = make_byte(false, false, false, false, false, 7);
        let info = parse(&[byte]).unwrap();
        assert_eq!(info.temporal_id, 7);
    }

    #[test]
    fn tid_mask_is_3_bits() {
        // TID=5 (binary 101) — make sure high bits don't bleed in
        let byte = make_byte(false, false, false, false, false, 5);
        let info = parse(&[byte]).unwrap();
        assert_eq!(info.temporal_id, 5);
    }

    #[test]
    fn multi_byte_uses_only_first_byte() {
        let byte0 = make_byte(true, false, true, false, false, 1);
        let info = parse(&[byte0, 0xFF, 0xAB]).unwrap();
        assert_eq!(info.temporal_id, 1);
        assert!(info.start_of_frame);
    }
}
