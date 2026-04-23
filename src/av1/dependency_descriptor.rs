//! AV1 Dependency Descriptor RTP header extension parser.
//!
//! Implements the minimal subset needed by a forwarding SFU: extract
//! `temporal_id` and `spatial_id` for the L3T3 (3 spatial × 3 temporal)
//! Chrome AV1-SVC profile. Unknown template IDs fall back to (0, 0).
//!
//! Reference: draft-ietf-payload-av1 §3.2.
//! See also: rheomesh `sfu/src/rtp/dependency_descriptor.rs` (MIT/Apache).

/// Extracted layer IDs from an AV1 Dependency Descriptor.
///
/// This struct may gain additional fields in future versions.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Av1DdInfo {
    /// Spatial layer (0 = base).
    pub spatial_id: u8,
    /// Temporal layer (0 = base, 1 = half-rate, 2 = quarter-rate in L3T3).
    pub temporal_id: u8,
    /// First packet of a frame.
    pub start_of_frame: bool,
    /// Last packet of a frame.
    pub end_of_frame: bool,
}

/// Parse the first byte of a raw AV1 Dependency Descriptor extension value.
///
/// Returns `None` if `bytes` is empty (malformed extension).
/// Unknown template IDs (>8 for L3T3) return `spatial_id = 0, temporal_id = 0`.
pub fn parse(bytes: &[u8]) -> Option<Av1DdInfo> {
    let first = *bytes.first()?;
    let start_of_frame = (first >> 7) & 1 == 1;
    let end_of_frame   = (first >> 6) & 1 == 1;
    let template_id    = first & 0x3F;

    // L3T3 layout: templates 0-8, spatial = id/3, temporal = id%3.
    // Unknown template IDs fall back to base layer (safe pass-through).
    let (spatial_id, temporal_id) = if template_id < 9 {
        (template_id / 3, template_id % 3)
    } else {
        (0, 0)
    };

    Some(Av1DdInfo { spatial_id, temporal_id, start_of_frame, end_of_frame })
}

#[cfg(test)]
mod tests {
    use super::*;

    // L3T3 template mapping (Chrome AV1 SVC default):
    //   template 0 → S0T0, 1 → S0T1, 2 → S0T2
    //   template 3 → S1T0, 4 → S1T1, 5 → S1T2
    //   template 6 → S2T0, 7 → S2T1, 8 → S2T2
    fn make_byte(sof: bool, eof: bool, template_id: u8) -> u8 {
        let sof_bit = if sof { 1u8 << 7 } else { 0 };
        let eof_bit = if eof { 1u8 << 6 } else { 0 };
        sof_bit | eof_bit | (template_id & 0x3F)
    }

    #[test]
    fn empty_returns_none() {
        assert!(parse(&[]).is_none());
    }

    #[test]
    fn base_layer_s0t0() {
        let byte = make_byte(true, false, 0); // template 0 → S0T0
        let info = parse(&[byte]).unwrap();
        assert_eq!(info.spatial_id, 0);
        assert_eq!(info.temporal_id, 0);
        assert!(info.start_of_frame);
        assert!(!info.end_of_frame);
    }

    #[test]
    fn s1t2_layer() {
        let byte = make_byte(false, true, 5); // template 5 → S1T2
        let info = parse(&[byte]).unwrap();
        assert_eq!(info.spatial_id, 1);
        assert_eq!(info.temporal_id, 2);
        assert!(!info.start_of_frame);
        assert!(info.end_of_frame);
    }

    #[test]
    fn s2t1_layer() {
        let byte = make_byte(true, true, 7); // template 7 → S2T1
        let info = parse(&[byte]).unwrap();
        assert_eq!(info.spatial_id, 2);
        assert_eq!(info.temporal_id, 1);
    }

    #[test]
    fn unknown_template_falls_back_to_base() {
        let byte = make_byte(false, false, 63); // unknown → S0T0
        let info = parse(&[byte]).unwrap();
        assert_eq!(info.spatial_id, 0);
        assert_eq!(info.temporal_id, 0);
    }
}
