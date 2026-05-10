use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

use super::meta::SceneMeta;

/// A scene as loaded from disk: its metadata and the GLSL source body
/// (NOT yet assembled with the prelude — that happens at GL compile time).
#[derive(Debug, Clone)]
pub struct LoadedScene {
    pub meta: SceneMeta,
    pub fragment_body: String,
    pub source_path: PathBuf,
}

/// In-memory registry of all scenes found in a directory.
#[derive(Debug, Default)]
pub struct SceneLibrary {
    scenes: BTreeMap<String, LoadedScene>,
}

impl SceneLibrary {
    pub fn load_dir(dir: &Path) -> Result<Self> {
        let mut lib = SceneLibrary::default();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("glsl") {
                continue;
            }
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let meta_path = path.with_extension("toml");
            if !meta_path.exists() {
                tracing::warn!("scene {} has no .toml metadata, skipping", path.display());
                continue;
            }
            let body = std::fs::read_to_string(&path)?;
            let meta_str = std::fs::read_to_string(&meta_path)?;
            let meta = SceneMeta::parse(&meta_str, &meta_path.display().to_string())?;
            meta.validate()?;
            lib.scenes.insert(
                stem,
                LoadedScene {
                    meta,
                    fragment_body: body,
                    source_path: path,
                },
            );
        }
        Ok(lib)
    }

    pub fn get(&self, name: &str) -> Option<&LoadedScene> {
        self.scenes.get(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.scenes.keys().map(|s| s.as_str())
    }

    pub fn require(&self, name: &str) -> Result<&LoadedScene> {
        self.get(name)
            .ok_or_else(|| Error::SceneNotFound(name.to_string()))
    }

    /// Replace one scene's body + meta (used by hot-reload).
    pub fn upsert(&mut self, name: &str, scene: LoadedScene) {
        self.scenes.insert(name.to_string(), scene);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_dir_picks_up_paired_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("test.glsl"),
            include_str!("../../tests/fixtures/good_scene.glsl"),
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("test.toml"),
            include_str!("../../tests/fixtures/good_scene.toml"),
        )
        .unwrap();
        let lib = SceneLibrary::load_dir(tmp.path()).unwrap();
        let s = lib.require("test").unwrap();
        assert_eq!(s.meta.params.len(), 2);
        assert!(s.fragment_body.contains("gl_FragColor"));
    }

    #[test]
    fn unpaired_glsl_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("orphan.glsl"), "void main() {}").unwrap();
        let lib = SceneLibrary::load_dir(tmp.path()).unwrap();
        assert!(lib.get("orphan").is_none());
    }

    #[test]
    fn missing_scene_errors() {
        let lib = SceneLibrary::default();
        let err = lib.require("nope").unwrap_err();
        assert!(matches!(err, Error::SceneNotFound(_)));
    }
}
