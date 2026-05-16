//! Raspberry Pi generation detection.
//!
//! Detects which Pi the binary is running on so the rest of the codebase can
//! gate features (per-scene `internal_resolution` caps, `min_pi_gen` filter,
//! install-time `render_scale` defaults). `MANDLEROT_PI_GEN=Pi5` overrides
//! the file-based detection — useful on the desktop dev box and for tests.
//!
//! Auto-scale (adaptive runtime fps feedback) is NOT part of this module —
//! see roadmap item 28 for that work.

use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub enum PiGen {
    Pi3,
    Pi4,
    Pi5,
    /// Desktop dev box, unrecognised Pi, or pre-Pi-3 model. Declared last so
    /// ordinal comparisons treat Unknown as "max tier — no filter, no caps".
    Unknown,
}

impl PiGen {
    /// Parse a device-tree model string. Loose substring match because
    /// firmware appends revision strings ("Raspberry Pi 5 Model B Rev 1.0").
    pub fn parse_model(s: &str) -> Self {
        let s = s.trim_end_matches('\0').trim();
        if s.contains("Raspberry Pi 5") {
            PiGen::Pi5
        } else if s.contains("Raspberry Pi 4") {
            PiGen::Pi4
        } else if s.contains("Raspberry Pi 3") {
            PiGen::Pi3
        } else {
            PiGen::Unknown
        }
    }

    /// Parse an env-var override (`MANDLEROT_PI_GEN=Pi5`). Returns None when
    /// the value is unrecognised so detection falls through to the file path.
    pub fn parse_env(s: &str) -> Option<Self> {
        match s.trim() {
            "Pi3" | "pi3" => Some(PiGen::Pi3),
            "Pi4" | "pi4" => Some(PiGen::Pi4),
            "Pi5" | "pi5" => Some(PiGen::Pi5),
            "Unknown" | "unknown" => Some(PiGen::Unknown),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PiGen::Pi3 => "Pi3",
            PiGen::Pi4 => "Pi4",
            PiGen::Pi5 => "Pi5",
            PiGen::Unknown => "Unknown",
        }
    }
}

static CACHED: OnceLock<PiGen> = OnceLock::new();

/// Detect the running Pi generation. Cached after the first call.
///
/// Resolution order:
///   1. `MANDLEROT_PI_GEN` env override.
///   2. `/proc/device-tree/model` substring match.
///   3. `PiGen::Unknown` (treated as "max tier — no filtering, no caps").
pub fn detect() -> PiGen {
    *CACHED.get_or_init(|| {
        if let Ok(val) = std::env::var("MANDLEROT_PI_GEN") {
            if let Some(gen) = PiGen::parse_env(&val) {
                return gen;
            }
        }
        match std::fs::read_to_string("/proc/device-tree/model") {
            Ok(s) => PiGen::parse_model(&s),
            Err(_) => PiGen::Unknown,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_model_pi3() {
        assert_eq!(
            PiGen::parse_model("Raspberry Pi 3 Model B Plus Rev 1.3"),
            PiGen::Pi3
        );
    }

    #[test]
    fn parse_model_pi4() {
        assert_eq!(
            PiGen::parse_model("Raspberry Pi 4 Model B Rev 1.5"),
            PiGen::Pi4
        );
    }

    #[test]
    fn parse_model_pi5() {
        assert_eq!(
            PiGen::parse_model("Raspberry Pi 5 Model B Rev 1.0\0"),
            PiGen::Pi5
        );
    }

    #[test]
    fn parse_model_unknown_falls_through() {
        assert_eq!(
            PiGen::parse_model("Raspberry Pi 2 Model B Rev 1.1"),
            PiGen::Unknown
        );
        assert_eq!(PiGen::parse_model(""), PiGen::Unknown);
        assert_eq!(PiGen::parse_model("Some Random Device"), PiGen::Unknown);
    }

    #[test]
    fn parse_env_recognises_known_values() {
        assert_eq!(PiGen::parse_env("Pi5"), Some(PiGen::Pi5));
        assert_eq!(PiGen::parse_env("pi3"), Some(PiGen::Pi3));
        assert_eq!(PiGen::parse_env("Unknown"), Some(PiGen::Unknown));
        assert_eq!(PiGen::parse_env("garbage"), None);
    }

    #[test]
    fn ord_lets_filtering_read_naturally() {
        // Filter rule used elsewhere: drop a scene when `scene.min_pi_gen >
        // detected`. With Unknown as the maximum, the desktop dev box never
        // filters anything — same behaviour as Pi 5.
        assert!(PiGen::Pi3 < PiGen::Pi5);
        assert!(PiGen::Pi4 < PiGen::Pi5);
        assert!(PiGen::Pi5 < PiGen::Unknown);
        assert!(PiGen::Pi3 < PiGen::Unknown);
    }
}
