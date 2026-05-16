//! LUT PNG → RGBA bytes → glow::Texture.
//!
//! LUT format: 256x16 RGBA strip. 16 slices of 16x16 each;
//! slice index = blue, x-within-slice = red, y = green.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use glow::HasContext;

use crate::error::{Error, Result};

/// Decode a 256x16 RGB/RGBA PNG into a tightly-packed RGBA byte buffer
/// (length = 256 * 16 * 4 = 16384). Returns Err on wrong size, unsupported
/// colour type, or malformed PNG bytes.
pub fn decode_lut_png(bytes: &[u8]) -> Result<Vec<u8>> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder
        .read_info()
        .map_err(|e| Error::Backend(format!("LUT png read_info: {e}")))?;
    let info = reader.info();
    if info.width != 256 || info.height != 16 {
        return Err(Error::Backend(format!(
            "LUT must be 256x16, got {}x{}",
            info.width, info.height
        )));
    }
    let bpp = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => {
            return Err(Error::Backend(format!(
                "LUT must be RGB or RGBA, got {other:?}"
            )))
        }
    };
    let mut raw = vec![0u8; reader.output_buffer_size()];
    reader
        .next_frame(&mut raw)
        .map_err(|e| Error::Backend(format!("LUT png next_frame: {e}")))?;
    let expected = 256usize * 16 * bpp;
    if raw.len() != expected {
        return Err(Error::Backend(format!(
            "LUT decoded buffer was {} bytes, expected {}",
            raw.len(),
            expected
        )));
    }
    if bpp == 4 {
        Ok(raw)
    } else {
        let mut out = Vec::with_capacity(256 * 16 * 4);
        for chunk in raw.chunks_exact(3) {
            out.extend_from_slice(chunk);
            out.push(255);
        }
        Ok(out)
    }
}

/// Read a LUT PNG file and decode it.
pub fn decode_lut_file(path: &Path) -> Result<Vec<u8>> {
    let mut f = File::open(path)?;
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes)?;
    decode_lut_png(&bytes)
}

/// Upload a 256x16 RGBA byte buffer to a freshly-allocated `glow::Texture`.
/// Sampling state: `GL_NEAREST` in both axes (the LUT shader manually
/// interpolates the B axis; bilinear would bleed across 16-pixel slices).
///
/// Returns the new texture. The caller owns it and must `gl.delete_texture`
/// when done.
pub fn upload_lut_texture(gl: &glow::Context, rgba: &[u8]) -> Result<glow::Texture> {
    if rgba.len() != 256 * 16 * 4 {
        return Err(Error::Backend(format!(
            "LUT buffer must be {} bytes, got {}",
            256 * 16 * 4,
            rgba.len()
        )));
    }
    unsafe {
        let tex = gl
            .create_texture()
            .map_err(|e| Error::Backend(format!("LUT create_texture: {e}")))?;
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA as i32,
            256,
            16,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            Some(rgba),
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::NEAREST as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::NEAREST as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.bind_texture(glow::TEXTURE_2D, None);
        Ok(tex)
    }
}

/// Convenience: decode + upload a LUT PNG file. Caller owns the texture.
pub fn load_lut_png(gl: &glow::Context, path: &Path) -> Result<glow::Texture> {
    let rgba = decode_lut_file(path)?;
    upload_lut_texture(gl, &rgba)
}

/// Enumerate `*.png` files directly inside `<dir>`, sorted lexically.
/// Non-existent dir → empty Vec (not an error).
pub fn scan_lut_paths(dir: &Path) -> Vec<std::path::PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<_> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("png"))
                .unwrap_or(false)
        })
        .collect();
    out.sort();
    out
}

/// Resolve a raw param-slot value to a valid index into a LUT vector.
/// Returns None if there are zero LUTs. Clamps non-finite, negative,
/// and over-large values to `[0, len-1]`.
pub fn pick_lut_index(slot_value: f32, lut_count: usize) -> Option<usize> {
    if lut_count == 0 {
        return None;
    }
    let last = lut_count - 1;
    let v = if slot_value.is_finite() { slot_value } else { 0.0 };
    let idx = v.max(0.0).round() as usize;
    Some(idx.min(last))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_test_png(w: u32, h: u32, channels: png::ColorType) -> Vec<u8> {
        let bpp = match channels {
            png::ColorType::Rgba => 4,
            png::ColorType::Rgb => 3,
            _ => panic!("test helper supports RGB/RGBA only"),
        };
        let data = vec![0u8; (w as usize) * (h as usize) * bpp];
        let mut buf = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut buf, w, h);
            enc.set_color(channels);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().unwrap();
            writer.write_image_data(&data).unwrap();
        }
        buf
    }

    #[test]
    fn decode_lut_png_accepts_256x16_rgba() {
        let bytes = encode_test_png(256, 16, png::ColorType::Rgba);
        let rgba = decode_lut_png(&bytes).expect("valid 256x16 RGBA should decode");
        assert_eq!(rgba.len(), 256 * 16 * 4);
    }

    #[test]
    fn decode_lut_png_accepts_256x16_rgb_and_pads_alpha() {
        let bytes = encode_test_png(256, 16, png::ColorType::Rgb);
        let rgba = decode_lut_png(&bytes).expect("valid 256x16 RGB should decode");
        assert_eq!(rgba.len(), 256 * 16 * 4);
        for px in rgba.chunks_exact(4) {
            assert_eq!(px[3], 255);
        }
    }

    #[test]
    fn decode_lut_png_rejects_wrong_size() {
        let bytes = encode_test_png(128, 128, png::ColorType::Rgba);
        let err = decode_lut_png(&bytes).expect_err("128x128 should be rejected");
        let msg = format!("{err}");
        assert!(msg.contains("256x16"), "msg={msg}");
    }

    #[test]
    fn decode_lut_png_rejects_gibberish() {
        let err = decode_lut_png(b"not a png").expect_err("gibberish must fail");
        let msg = format!("{err}");
        assert!(msg.contains("LUT png read_info"), "msg={msg}");
    }

    #[test]
    fn pick_lut_index_returns_none_for_empty() {
        assert_eq!(pick_lut_index(0.0, 0), None);
        assert_eq!(pick_lut_index(5.0, 0), None);
    }

    #[test]
    fn pick_lut_index_clamps_overflow_to_last() {
        assert_eq!(pick_lut_index(99.0, 3), Some(2));
    }

    #[test]
    fn pick_lut_index_clamps_negative_to_zero() {
        assert_eq!(pick_lut_index(-1.0, 4), Some(0));
    }

    #[test]
    fn pick_lut_index_rounds_fractional() {
        assert_eq!(pick_lut_index(1.4, 4), Some(1));
        assert_eq!(pick_lut_index(1.6, 4), Some(2));
    }

    #[test]
    fn pick_lut_index_handles_nan() {
        assert_eq!(pick_lut_index(f32::NAN, 3), Some(0));
    }

    #[test]
    fn scan_lut_paths_returns_empty_for_missing_dir() {
        let p = std::path::PathBuf::from("/nope/does/not/exist/anywhere");
        assert!(scan_lut_paths(&p).is_empty());
    }

    #[test]
    fn scan_lut_paths_finds_pngs_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        for name in ["zeta.png", "alpha.png", "ignore.txt", "beta.PNG"] {
            std::fs::write(tmp.path().join(name), b"x").unwrap();
        }
        let got = scan_lut_paths(tmp.path());
        let names: Vec<String> = got
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["alpha.png", "beta.PNG", "zeta.png"]);
    }
}
