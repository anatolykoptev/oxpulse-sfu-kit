//! Video Frame Marking RTP header extension (`feature = "vfm"`).
//!
//! Implements the 1-byte short form of RFC 9626 (March 2025), which exposes
//! temporal layer ID and frame metadata for H.264, VP9, and HEVC without
//! full bitstream parsing.

pub mod frame_marking;
pub use frame_marking::FrameMarkingInfo;
