//! AV1 codec support (`feature = "av1-dd"`).
//!
//! Currently limited to Dependency Descriptor (DD) RTP header extension
//! parsing for temporal/spatial layer identification.

pub mod dependency_descriptor;
pub use dependency_descriptor::Av1DdInfo;
