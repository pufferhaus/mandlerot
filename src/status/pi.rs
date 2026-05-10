//! Pi ILI9486 SPI backend.
//!
//! Wiring (Pi 3 BCM pin numbers):
//!   SCLK   GPIO11 (SPI0 SCLK)   pin 23
//!   MOSI   GPIO10               pin 19
//!   CS     GPIO8  (CE0)         pin 24
//!   DC     GPIO24               pin 18
//!   RST    GPIO25               pin 22
//!   BLK    GPIO18 (always on)   pin 12

#![cfg(all(feature = "pi", target_os = "linux"))]

use std::time::Duration;

use display_interface_spi::SPIInterface;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use mipidsi::models::ILI9486Rgb565;
use mipidsi::Builder;
use rppal::gpio::{Gpio, OutputPin};
use rppal::spi::{Bus, Mode, SlaveSelect, Spi};

use crate::error::{Error, Result};

use super::render::Fb;

pub struct PiPanelBackend {
    display: mipidsi::Display<SPIInterface<Spi, OutputPin>, ILI9486Rgb565, OutputPin>,
}

impl PiPanelBackend {
    pub fn open() -> Result<Self> {
        let gpio = Gpio::new().map_err(|e| Error::Backend(format!("gpio: {e}")))?;
        let mut rst = gpio
            .get(25)
            .map_err(|e| Error::Backend(format!("gpio25: {e}")))?
            .into_output();
        let dc = gpio
            .get(24)
            .map_err(|e| Error::Backend(format!("gpio24: {e}")))?
            .into_output();
        let mut blk = gpio
            .get(18)
            .map_err(|e| Error::Backend(format!("gpio18: {e}")))?
            .into_output();
        blk.set_high();

        let spi = Spi::new(Bus::Spi0, SlaveSelect::Ss0, 32_000_000, Mode::Mode0)
            .map_err(|e| Error::Backend(format!("spi open: {e}")))?;

        let di = SPIInterface::new(spi, dc);
        let mut delay = Delay;
        let display = Builder::new(ILI9486Rgb565, di)
            .reset_pin(rst)
            .display_size(480, 320)
            .orientation(mipidsi::options::Orientation::default())
            .init(&mut delay)
            .map_err(|e| Error::Backend(format!("ili9486 init: {e:?}")))?;

        Ok(Self { display })
    }
}

impl super::Backend for PiPanelBackend {
    fn flush_full(&mut self, fb: &Fb) -> Result<()> {
        let area = Rectangle::new(Point::new(0, 0), Size::new(fb.width, fb.height));
        let pixels = fb.data.iter().copied();
        self.display
            .fill_contiguous(&area, pixels)
            .map_err(|_| Error::Backend("display fill_contiguous failed".into()))
    }

    fn flush_runs(&mut self, fb: &Fb, runs: &[(usize, usize, usize)]) -> Result<()> {
        // Group runs by row span and push as small Rectangles.
        for &(row, col_lo, col_hi) in runs {
            let cell_w = super::glyphs::CELL_W;
            let cell_h = super::glyphs::CELL_H;
            let x = (col_lo as u32) * cell_w;
            let y = (row as u32) * cell_h;
            let w = ((col_hi - col_lo) as u32) * cell_w;
            let h = cell_h;
            let area = Rectangle::new(Point::new(x as i32, y as i32), Size::new(w, h));
            // Collect pixel slice rectangle.
            let mut pixels: Vec<Rgb565> = Vec::with_capacity((w * h) as usize);
            for yy in y..(y + h) {
                for xx in x..(x + w) {
                    pixels.push(fb.pixel_at(xx, yy));
                }
            }
            self.display
                .fill_contiguous(&area, pixels.into_iter())
                .map_err(|_| Error::Backend("partial flush failed".into()))?;
        }
        Ok(())
    }
}

struct Delay;
impl embedded_hal::delay::DelayNs for Delay {
    fn delay_ns(&mut self, ns: u32) {
        std::thread::sleep(Duration::from_nanos(ns as u64));
    }
}
