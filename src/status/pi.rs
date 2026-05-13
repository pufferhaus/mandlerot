//! Pi status panel backend via the kernel `fb_ili9486` framebuffer.
//!
//! The MPI3501 / generic 3.5" SPI ILI9486 panel is driven by the upstream
//! `fbtft` driver. Configure via /boot/firmware/config.txt:
//!
//! ```
//! dtoverlay=fbtft,spi0-0,ili9486,reset_pin=25,dc_pin=24,led_pin=18,
//!     width=320,height=480,rotate=0,bgr=1,speed=32000000,fps=30
//! ```
//!
//! `rotate=0` is intentional: the kernel driver's address-window math is
//! buggy on this clone panel for landscape rotations, so we leave the
//! framebuffer in native portrait (320×480) and rotate 90° CW in software
//! when copying our landscape (480×320) status grid into it.

#![cfg(all(feature = "pi", target_os = "linux"))]

use std::fs::{File, OpenOptions};
use std::path::PathBuf;

use embedded_graphics::pixelcolor::IntoStorage;
use memmap2::{MmapMut, MmapOptions};

use crate::error::{Error, Result};

use super::render::{Fb, PANEL_H, PANEL_W};

const PANEL_NAME: &str = "fb_ili9486";
const NATIVE_W: usize = PANEL_H as usize; // 320
const NATIVE_H: usize = PANEL_W as usize; // 480
const BYTES_PER_PIXEL: usize = 2; // Rgb565

pub struct PiPanelBackend {
    _file: File,
    map: MmapMut,
}

impl PiPanelBackend {
    pub fn open() -> Result<Self> {
        let path = find_panel_fb()?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|e| Error::Backend(format!("open {}: {e}", path.display())))?;
        let map = unsafe {
            MmapOptions::new()
                .len(NATIVE_W * NATIVE_H * BYTES_PER_PIXEL)
                .map_mut(&file)
                .map_err(|e| Error::Backend(format!("mmap {}: {e}", path.display())))?
        };
        Ok(Self { _file: file, map })
    }
}

/// Find the framebuffer device whose `name` matches the fb_ili9486 driver.
/// Walks `/sys/class/graphics/fb*/name`; returns the matching `/dev/fbN`.
fn find_panel_fb() -> Result<PathBuf> {
    for entry in std::fs::read_dir("/sys/class/graphics")
        .map_err(|e| Error::Backend(format!("read /sys/class/graphics: {e}")))?
    {
        let entry = entry.map_err(|e| Error::Backend(format!("dir entry: {e}")))?;
        let name_path = entry.path().join("name");
        let Ok(name) = std::fs::read_to_string(&name_path) else {
            continue;
        };
        if name.trim() == PANEL_NAME {
            let dev_name = entry.file_name();
            return Ok(PathBuf::from("/dev").join(&dev_name));
        }
    }
    Err(Error::Backend(format!(
        "no /sys/class/graphics fb with name '{PANEL_NAME}' found"
    )))
}

/// Swap the R and B 5-bit fields of an Rgb565 word. The MPI3501 panel's
/// ILI9486 reads incoming top-5 bits as the blue channel and bottom-5 as
/// red — opposite of the embedded-graphics RGB565 convention. The fbtft
/// `bgr=1` overlay flag tries to fix this in the driver but produces
/// odd bit-mixing on this clone; doing the swap here is clean.
#[inline]
fn swap_rb565(raw: u16) -> u16 {
    ((raw & 0x001F) << 11) | (raw & 0x07E0) | ((raw & 0xF800) >> 11)
}

/// Write one source landscape pixel into the rotated native portrait
/// framebuffer at the equivalent CW-90° location.
#[inline]
fn write_rotated(map: &mut [u8], sx: usize, sy: usize, pixel_le: [u8; 2]) {
    // 90° CW: dst_x = NATIVE_W - 1 - sy, dst_y = sx.
    let dx = NATIVE_W - 1 - sy;
    let dy = sx;
    let off = (dy * NATIVE_W + dx) * BYTES_PER_PIXEL;
    map[off] = pixel_le[0];
    map[off + 1] = pixel_le[1];
}

impl super::Backend for PiPanelBackend {
    fn flush_full(&mut self, fb: &Fb) -> Result<()> {
        let map = &mut self.map[..];
        let w = fb.width as usize;
        let h = fb.height as usize;
        for sy in 0..h {
            let row_start = sy * w;
            for sx in 0..w {
                let raw: u16 = swap_rb565(fb.data[row_start + sx].into_storage());
                write_rotated(map, sx, sy, raw.to_le_bytes());
            }
        }
        Ok(())
    }

    fn flush_runs(&mut self, fb: &Fb, runs: &[(usize, usize, usize)]) -> Result<()> {
        let map = &mut self.map[..];
        let cell_w = super::glyphs::CELL_W as usize;
        let cell_h = super::glyphs::CELL_H as usize;
        for &(row, col_lo, col_hi) in runs {
            let x0 = col_lo * cell_w;
            let y0 = row * cell_h;
            let w = (col_hi - col_lo) * cell_w;
            for sy in y0..(y0 + cell_h) {
                for sx in x0..(x0 + w) {
                    let raw: u16 = swap_rb565(fb.pixel_at(sx as u32, sy as u32).into_storage());
                    write_rotated(map, sx, sy, raw.to_le_bytes());
                }
            }
        }
        Ok(())
    }
}
