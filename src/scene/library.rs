use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::platform::PiGen;

use super::meta::SceneMeta;

/// A scene as loaded from disk: its metadata and the GLSL source body
/// (NOT yet assembled with the prelude — that happens at GL compile time).
#[derive(Debug, Clone)]
pub struct LoadedScene {
    pub meta: SceneMeta,
    pub fragment_body: String,
    pub source_path: PathBuf,
    pub is_hq: bool,
}

/// In-memory registry of all scenes found in a directory.
#[derive(Debug, Default, Clone)]
pub struct SceneLibrary {
    scenes: BTreeMap<String, LoadedScene>,
    /// Count of scenes parsed but dropped because their `min_pi_gen` exceeds
    /// the detected gen. Surfaced in the scene-list menu.
    filtered_count: usize,
}

impl SceneLibrary {
    /// Load all paired `*.glsl` + `*.toml` files in `dir`. No Pi-gen
    /// filtering — equivalent to running on the desktop dev box.
    pub fn load_dir(dir: &Path) -> Result<Self> {
        Self::load_dir_for_gen(dir, PiGen::Unknown)
    }

    /// Load all paired `*.glsl` + `*.toml` files in `dir`, dropping scenes
    /// whose `min_pi_gen` exceeds `detected`. `PiGen::Unknown` disables
    /// filtering (desktop dev). See roadmap item 28a.
    pub fn load_dir_for_gen(dir: &Path, detected: PiGen) -> Result<Self> {
        let mut lib = SceneLibrary::default();
        lib.inject_baked_scenes();
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
            if let Some(required) = meta.min_pi_gen {
                if required > detected {
                    tracing::info!(
                        "scene {} requires {} (detected {}); filtered",
                        stem,
                        required.as_str(),
                        detected.as_str(),
                    );
                    lib.filtered_count += 1;
                    continue;
                }
            }
            // Probe for an HQ variant: `foo.hq.glsl` alongside `foo.glsl`.
            let hq_path = path.with_file_name(format!("{}.hq.glsl", stem));
            let (fragment_body, is_hq) = if detected >= PiGen::Pi4 && hq_path.exists() {
                let hq_body = std::fs::read_to_string(&hq_path)?;
                tracing::debug!("scene {}: loaded HQ variant ({:?})", stem, hq_path);
                (hq_body, true)
            } else {
                (body, false)
            };
            lib.scenes.insert(
                stem,
                LoadedScene {
                    meta,
                    fragment_body,
                    source_path: if is_hq { hq_path } else { path },
                    is_hq,
                },
            );
        }
        Ok(lib)
    }

    /// Number of scenes dropped during `load_dir_for_gen` because their
    /// `min_pi_gen` exceeded the detected gen.
    pub fn filtered_count(&self) -> usize {
        self.filtered_count
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

    /// Inject the baked-in fallback scenes (`__safe__` SMPTE bars and
    /// `__video__` live feed sampler). Called automatically by
    /// `load_dir_for_gen`. Used by PANIC, by the operator binding
    /// `__video__` to a slot, and as a last-resort render target.
    /// Cannot be removed via hot-reload.
    pub fn inject_baked_scenes(&mut self) {
        let safe_meta = SceneMeta::parse(
            "name = \"__safe__\"\ndisplay_name = \"Safe Fallback\"\n",
            "<baked>",
        )
        .expect("baked safe-scene meta must parse");
        self.scenes.insert(
            "__safe__".to_string(),
            LoadedScene {
                meta: safe_meta,
                fragment_body: crate::render::shader::SAFE_SCENE.to_string(),
                source_path: PathBuf::from("<baked>"),
                is_hq: false,
            },
        );

        let video_meta = SceneMeta::parse(
            "name = \"__video__\"\ndisplay_name = \"Video In\"\nkeyable = true\n",
            "<baked>",
        )
        .expect("baked video-scene meta must parse");
        self.scenes.insert(
            "__video__".to_string(),
            LoadedScene {
                meta: video_meta,
                fragment_body: crate::render::shader::VIDEO_SCENE.to_string(),
                source_path: PathBuf::from("<baked>"),
                is_hq: false,
            },
        );
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

    /// `min_pi_gen` filters scenes that require a higher gen than detected.
    /// Same scene loads fine when detected gen is high enough. Tests both
    /// directions through `load_dir_for_gen`.
    #[test]
    fn min_pi_gen_filters_on_lower_detected_gen() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("future.glsl"),
            include_str!("../../tests/fixtures/good_scene.glsl"),
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("future.toml"),
            "name = \"future\"\nmin_pi_gen = \"Pi5\"\n",
        )
        .unwrap();

        let lib_pi3 = SceneLibrary::load_dir_for_gen(tmp.path(), PiGen::Pi3).unwrap();
        assert!(lib_pi3.get("future").is_none());
        assert_eq!(lib_pi3.filtered_count(), 1);

        let lib_pi5 = SceneLibrary::load_dir_for_gen(tmp.path(), PiGen::Pi5).unwrap();
        assert!(lib_pi5.get("future").is_some());
        assert_eq!(lib_pi5.filtered_count(), 0);

        // Unknown (desktop dev) behaves as the maximum tier: no filtering.
        let lib_dev = SceneLibrary::load_dir_for_gen(tmp.path(), PiGen::Unknown).unwrap();
        assert!(lib_dev.get("future").is_some());
        assert_eq!(lib_dev.filtered_count(), 0);
    }

    #[test]
    fn hq_variant_loaded_on_pi4_plus() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("mystic.glsl"),
            "void main() { gl_FragColor = vec4(1.0); }",
        ).unwrap();
        std::fs::write(
            tmp.path().join("mystic.toml"),
            "name = \"mystic\"\n",
        ).unwrap();
        std::fs::write(
            tmp.path().join("mystic.hq.glsl"),
            "void main() { fragColor = vec4(1.0); }",
        ).unwrap();

        let lib3 = SceneLibrary::load_dir_for_gen(tmp.path(), PiGen::Pi3).unwrap();
        let s3 = lib3.require("mystic").unwrap();
        assert!(!s3.is_hq);
        assert!(s3.fragment_body.contains("gl_FragColor"));

        let lib4 = SceneLibrary::load_dir_for_gen(tmp.path(), PiGen::Pi4).unwrap();
        let s4 = lib4.require("mystic").unwrap();
        assert!(s4.is_hq);
        assert!(s4.fragment_body.contains("fragColor"));

        let libu = SceneLibrary::load_dir_for_gen(tmp.path(), PiGen::Unknown).unwrap();
        let su = libu.require("mystic").unwrap();
        assert!(su.is_hq);
    }

    #[test]
    fn hq_variant_not_loaded_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("plain.glsl"),
            "void main() { gl_FragColor = vec4(1.0); }",
        ).unwrap();
        std::fs::write(
            tmp.path().join("plain.toml"),
            "name = \"plain\"\n",
        ).unwrap();
        let lib = SceneLibrary::load_dir_for_gen(tmp.path(), PiGen::Pi4).unwrap();
        let s = lib.require("plain").unwrap();
        assert!(!s.is_hq);
    }

    #[test]
    fn baked_video_scene_is_present_regardless_of_gen() {
        let tmp = tempfile::tempdir().unwrap();
        // Empty dir — only baked scenes should be present.
        let lib = SceneLibrary::load_dir_for_gen(tmp.path(), PiGen::Pi3).unwrap();
        assert!(lib.get("__safe__").is_some());
        assert!(lib.get("__video__").is_some());
        let v = lib.get("__video__").unwrap();
        assert_eq!(v.meta.display_name.as_deref(), Some("Video In"));
        assert_eq!(v.fragment_body, crate::render::shader::VIDEO_SCENE);
    }
}
