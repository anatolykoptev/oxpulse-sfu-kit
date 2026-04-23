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
    let end_of_frame = (first >> 6) & 1 == 1;
    let template_id = first & 0x3F;

    // L3T3 layout: templates 0-8, spatial = id/3, temporal = id%3.
    // Unknown template IDs fall back to base layer (safe pass-through).
    let (spatial_id, temporal_id) = if template_id < 9 {
        (template_id / 3, template_id % 3)
    } else {
        (0, 0)
    };

    Some(Av1DdInfo {
        spatial_id,
        temporal_id,
        start_of_frame,
        end_of_frame,
    })
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

    #[test]
    fn all_nine_l3t3_templates_map_correctly() {
        // Full L3T3 grid: template_id = spatial*3 + temporal
        let expected: [(u8, u8); 9] = [
            (0, 0), // S0T0
            (0, 1), // S0T1
            (0, 2), // S0T2
            (1, 0), // S1T0
            (1, 1), // S1T1
            (1, 2), // S1T2
            (2, 0), // S2T0
            (2, 1), // S2T1
            (2, 2), // S2T2
        ];
        for (template_id, (exp_spatial, exp_temporal)) in expected.iter().enumerate() {
            let byte = make_byte(false, false, template_id as u8);
            let info = parse(&[byte]).unwrap();
            assert_eq!(
                info.spatial_id, *exp_spatial,
                "template {template_id}: expected spatial {exp_spatial}, got {}",
                info.spatial_id
            );
            assert_eq!(
                info.temporal_id, *exp_temporal,
                "template {template_id}: expected temporal {exp_temporal}, got {}",
                info.temporal_id
            );
        }
    }

    #[test]
    fn template_id_8_is_last_valid_l3t3() {
        // template 8 = S2T2; template 9 = fallback (0,0)
        let byte8 = make_byte(false, false, 8);
        let info8 = parse(&[byte8]).unwrap();
        assert_eq!(
            (info8.spatial_id, info8.temporal_id),
            (2, 2),
            "template 8 must map to S2T2"
        );

        let byte9 = make_byte(false, false, 9);
        let info9 = parse(&[byte9]).unwrap();
        assert_eq!(
            (info9.spatial_id, info9.temporal_id),
            (0, 0),
            "template 9 is out of L3T3 range, must fall back to S0T0"
        );
    }

    #[test]
    fn multi_byte_payload_uses_only_first_byte() {
        // DD payloads can be multi-byte; we only parse byte 0
        let payload = [make_byte(true, true, 4), 0xFF, 0xFF, 0xFF];
        let info = parse(&payload).unwrap();
        // template 4 = S1T1
        assert_eq!(info.spatial_id, 1);
        assert_eq!(info.temporal_id, 1);
        assert!(info.start_of_frame);
        assert!(info.end_of_frame);
    }

    #[test]
    fn template_id_63_highest_unknown_is_base() {
        // Already tested but make the boundary explicit: 0x3F = 63, highest 6-bit value
        let byte = make_byte(true, false, 0x3F);
        let info = parse(&[byte]).unwrap();
        assert_eq!((info.spatial_id, info.temporal_id), (0, 0));
    }
}
