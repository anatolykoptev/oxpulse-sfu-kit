//! Compile-time-ish guard that the public API does not expose `str0m::` types.
//!
//! Greps public item signatures in source files, allowlisting the documented
//! escape hatches (`src/raw.rs` and the single `SfuRtc::from_raw` constructor).

use std::fs;
use std::path::Path;

/// Public files to scan. The `raw` module and test-only files are intentionally excluded.
const FILES_TO_SCAN: &[&str] = &[
    "src/lib.rs",
    "src/bandwidth.rs",
    "src/config.rs",
    "src/fanout.rs",
    "src/ids.rs",
    "src/keyframe.rs",
    "src/media.rs",
    "src/net.rs",
    "src/propagate.rs",
    "src/rtcp_stats.rs",
    "src/rtc.rs",
    "src/udp_loop.rs",
    "src/client/mod.rs",
    "src/client/accessors.rs",
    "src/client/construct.rs",
    "src/client/fanout.rs",
    "src/client/keyframe.rs",
    "src/client/layer.rs",
    "src/client/stats.rs",
    "src/client/tracks.rs",
    "src/metrics/mod.rs",
    "src/metrics/prom.rs",
    "src/metrics/noop.rs",
    "src/registry/mod.rs",
    "src/registry/drive.rs",
    "src/registry/lifecycle.rs",
];

/// Lines that contain `str0m::` AND are part of a public signature MUST match
/// one of these — they are documented exceptions or false positives that are
/// actually in inner impl bodies.
const ALLOWLIST_SUBSTRINGS: &[&str] = &[
    // The documented escape-hatch constructor:
    "pub fn from_raw(rtc: str0m::Rtc) -> Self {",
];

#[test]
fn no_str0m_in_public_api_surface() {
    let mut violations: Vec<String> = Vec::new();

    for path in FILES_TO_SCAN {
        if !Path::new(path).exists() {
            panic!(
                "encapsulation_surface test config drift: {path} not found — update FILES_TO_SCAN"
            );
        }
        let contents = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));

        for (idx, raw_line) in contents.lines().enumerate() {
            let line = raw_line.trim_start();
            let line_no = idx + 1;

            // Quick reject: not a public item declaration
            let is_pub_signature = line.starts_with("pub fn ")
                || line.starts_with("pub struct ")
                || line.starts_with("pub enum ")
                || line.starts_with("pub type ")
                || line.starts_with("pub const ")
                || line.starts_with("pub static ");
            if !is_pub_signature {
                continue;
            }

            // pub(crate) / pub(super) are not part of the external public API — skip
            if line.starts_with("pub(") {
                continue;
            }

            if !line.contains("str0m::") {
                continue;
            }

            // --- False-positive filters ---

            // Tuple-struct definitions: `pub struct Foo(InnerType)` — the inner
            // type is private (no `pub` on the tuple field). Only a leak if the
            // field itself is declared `pub` inside the parens, i.e. the pattern
            // `pub struct Foo(pub str0m::`. Plain `pub struct Foo(str0m::` is safe.
            if line.starts_with("pub struct ") && line.contains('(') {
                // Find what's inside the parens and check for `pub str0m::`
                let paren_content = line.split_once('(').map_or("", |(_, r)| r);
                if !paren_content.contains("pub str0m::") {
                    continue;
                }
                // Also skip if the only pub field is pub(crate)
                if paren_content.contains("pub(crate) str0m::")
                    && !paren_content.contains("pub str0m::")
                {
                    continue;
                }
            }

            // Const/static: `pub const FOO: Self = Self(str0m::...)` — the TYPE
            // is `Self`, str0m appears only in the initializer value. Skip when
            // the declared type (between `:` and `=`) does not contain `str0m::`.
            if line.starts_with("pub const ") || line.starts_with("pub static ") {
                // Extract the declared type: everything between the first `:` and `=`
                if let Some((_, after_colon)) = line.split_once(':') {
                    let declared_type = after_colon
                        .split_once('=')
                        .map_or(after_colon, |(l, _)| l)
                        .trim();
                    if !declared_type.contains("str0m::") {
                        continue; // str0m:: only in initializer, not in the type
                    }
                }
            }

            // Allowlist check
            if ALLOWLIST_SUBSTRINGS
                .iter()
                .any(|allowed| line.contains(allowed))
            {
                continue;
            }

            violations.push(format!("{path}:{line_no}: {}", raw_line.trim()));
        }
    }

    if !violations.is_empty() {
        panic!(
            "Public API exposes str0m types (encapsulation regression):\n{}\n\n\
             If this is intentional, add to the ALLOWLIST_SUBSTRINGS list in this test.",
            violations.join("\n")
        );
    }
}

#[test]
fn raw_module_contains_expected_exports() {
    // Positive guard: raw.rs MUST re-export RawRtc and RawRtcConfig.
    let contents = fs::read_to_string("src/raw.rs").expect("read src/raw.rs");
    assert!(
        contents.contains("pub use str0m::Rtc as RawRtc"),
        "src/raw.rs must re-export str0m::Rtc as RawRtc"
    );
    assert!(
        contents.contains("pub use str0m::RtcConfig as RawRtcConfig"),
        "src/raw.rs must re-export str0m::RtcConfig as RawRtcConfig"
    );
}
