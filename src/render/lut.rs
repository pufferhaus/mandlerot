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
/// # Safety
///
/// Caller must have a current GL context. The returned texture is owned
/// by the caller — they're responsible for calling `gl.delete_texture` on it.
pub fn upload_lut_texture(gl: &glow::Context, rgba: &[u8]) -> Result<glow::Texture> {
    assert_eq!(rgba.len(), 256 * 16 * 4, "LUT buffer must be 256*16*4 bytes");
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
}
