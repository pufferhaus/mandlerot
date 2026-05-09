use serde::Deserialize;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Deserialize)]
pub struct SceneMeta {
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub internal_resolution: Option<String>,
    #[serde(default)]
    pub params: Vec<ParamDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParamDef {
    pub slot: u8,
    pub name: String,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    #[serde(default = "default_curve")]
    pub curve: Curve,
    #[serde(default)]
    pub audio_route: AudioRoute,
    #[serde(default)]
    pub audio_amount: f32,
    #[serde(default = "default_polarity")]
    pub audio_polarity: f32,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Curve {
    Linear,
    Exp,
    Log,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AudioRoute {
    #[default]
    None,
    Bass,
    Lomid,
    Himid,
    Treble,
    Beat,
}

fn default_curve() -> Curve {
    Curve::Linear
}
fn default_polarity() -> f32 {
    1.0
}

impl SceneMeta {
    pub fn parse(s: &str, file_label: &str) -> Result<Self> {
        toml::from_str(s).map_err(|e| Error::SceneMeta {
            file: file_label.to_string(),
            source: e,
        })
    }

    /// Validate cross-field constraints (slot uniqueness, range sanity).
    pub fn validate(&self) -> Result<()> {
        let mut seen = [false; 8];
        for p in &self.params {
            if p.slot >= 8 {
                return Err(Error::ShaderCompile(format!(
                    "param slot {} out of range (must be 0-7)",
                    p.slot
                )));
            }
            if seen[p.slot as usize] {
                return Err(Error::ShaderCompile(format!(
                    "duplicate param slot {}",
                    p.slot
                )));
            }
            seen[p.slot as usize] = true;
            if p.min >= p.max {
                return Err(Error::ShaderCompile(format!(
                    "param {} has min >= max",
                    p.name
                )));
            }
            if p.default < p.min || p.default > p.max {
                return Err(Error::ShaderCompile(format!(
                    "param {} default outside [min, max]",
                    p.name
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> &'static str {
        include_str!("../../tests/fixtures/good_scene.toml")
    }

    fn bad() -> &'static str {
        include_str!("../../tests/fixtures/bad_meta.toml")
    }

    #[test]
    fn parses_good_scene() {
        let m = SceneMeta::parse(good(), "good_scene.toml").unwrap();
        assert_eq!(m.name, "test_scene");
        assert_eq!(m.params.len(), 2);
        assert_eq!(m.params[0].name, "zoom");
        assert_eq!(m.params[0].curve, Curve::Exp);
        assert_eq!(m.params[0].audio_route, AudioRoute::Bass);
        assert!(m.validate().is_ok());
    }

    #[test]
    fn rejects_bad_toml() {
        let err = SceneMeta::parse(bad(), "bad.toml").unwrap_err();
        assert!(matches!(err, Error::SceneMeta { .. }));
    }

    #[test]
    fn validate_catches_duplicate_slot() {
        let s = r#"
            name = "x"
            [[params]]
            slot = 0
            name = "a"
            min = 0.0
            max = 1.0
            default = 0.5
            [[params]]
            slot = 0
            name = "b"
            min = 0.0
            max = 1.0
            default = 0.5
        "#;
        let m = SceneMeta::parse(s, "x").unwrap();
        let err = m.validate().unwrap_err();
        assert!(err.to_string().contains("duplicate param slot 0"));
    }

    #[test]
    fn validate_catches_min_gte_max() {
        let s = r#"
            name = "x"
            [[params]]
            slot = 0
            name = "a"
            min = 1.0
            max = 1.0
            default = 1.0
        "#;
        let m = SceneMeta::parse(s, "x").unwrap();
        let err = m.validate().unwrap_err();
        assert!(err.to_string().contains("min >= max"));
    }

    #[test]
    fn audio_route_default_is_none() {
        let s = r#"
            name = "x"
            [[params]]
            slot = 0
            name = "a"
            min = 0.0
            max = 1.0
            default = 0.5
        "#;
        let m = SceneMeta::parse(s, "x").unwrap();
        assert_eq!(m.params[0].audio_route, AudioRoute::None);
    }
}
