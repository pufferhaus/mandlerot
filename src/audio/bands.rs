//! Hann window + real FFT + 4-band perceptual binning.

use std::sync::Arc;

use rustfft::{num_complex::Complex32, FftPlanner};

pub const FFT_SIZE: usize = 1024;
pub const SAMPLE_RATE: f32 = 44100.0;

#[derive(Debug, Clone, Copy)]
pub struct BandRanges {
    pub bass: (usize, usize),
    pub lomid: (usize, usize),
    pub himid: (usize, usize),
    pub treble: (usize, usize),
}

impl BandRanges {
    pub fn for_44100_1024() -> Self {
        // bin width = 44100/1024 ≈ 43.07 Hz. Bin 0 is DC, exclude it.
        let bin_for = |hz: f32| (hz / (SAMPLE_RATE / FFT_SIZE as f32)).round() as usize;
        Self {
            bass: (bin_for(20.0).max(1), bin_for(200.0)),
            lomid: (bin_for(200.0), bin_for(800.0)),
            himid: (bin_for(800.0), bin_for(3200.0)),
            treble: (bin_for(3200.0), bin_for(20000.0).min(FFT_SIZE / 2 - 1)),
        }
    }
}

pub struct BandBinner {
    fft: Arc<dyn rustfft::Fft<f32>>,
    window: Vec<f32>,
    ranges: BandRanges,
}

impl BandBinner {
    pub fn new() -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let window = hann(FFT_SIZE);
        Self {
            fft,
            window,
            ranges: BandRanges::for_44100_1024(),
        }
    }

    /// Returns 4 normalized band magnitudes (mean log magnitude per band).
    /// Magnitudes are NOT normalized to [0,1] — that's the envelope follower's job.
    pub fn process(&self, samples: &[f32; FFT_SIZE]) -> [f32; 4] {
        let (bands, _mags) = self.process_with_mags(samples);
        bands
    }

    /// Same as `process` but also returns the linear FFT magnitudes (one per
    /// bin in the lower half of the spectrum). Useful for downstream
    /// consumers like the spectral-flux beat detector that need raw
    /// per-bin magnitudes rather than the 4-band reduction.
    pub fn process_with_mags(&self, samples: &[f32; FFT_SIZE]) -> ([f32; 4], Vec<f32>) {
        let mut buf: Vec<Complex32> = samples
            .iter()
            .zip(&self.window)
            .map(|(s, w)| Complex32::new(s * w, 0.0))
            .collect();
        self.fft.process(&mut buf);
        let mags: Vec<f32> = buf
            .iter()
            .take(FFT_SIZE / 2)
            .map(|c| (c.re * c.re + c.im * c.im).sqrt())
            .collect();
        let bass = mean_log_mag(&mags, self.ranges.bass);
        let lomid = mean_log_mag(&mags, self.ranges.lomid);
        let himid = mean_log_mag(&mags, self.ranges.himid);
        let treble = mean_log_mag(&mags, self.ranges.treble);
        ([bass, lomid, himid, treble], mags)
    }
}

impl Default for BandBinner {
    fn default() -> Self {
        Self::new()
    }
}

fn hann(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = std::f32::consts::PI * i as f32 / (n - 1) as f32;
            x.sin().powi(2)
        })
        .collect()
}

fn mean_log_mag(mags: &[f32], range: (usize, usize)) -> f32 {
    let (lo, hi) = range;
    if hi <= lo {
        return 0.0;
    }
    let slice = &mags[lo..hi.min(mags.len())];
    if slice.is_empty() {
        return 0.0;
    }
    let sum: f32 = slice.iter().map(|m| (m + 1e-9).ln()).sum();
    sum / slice.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn sine(freq_hz: f32, n: usize) -> [f32; FFT_SIZE] {
        let mut out = [0.0; FFT_SIZE];
        for (i, sample) in out.iter_mut().enumerate().take(n.min(FFT_SIZE)) {
            *sample = (TAU * freq_hz * i as f32 / SAMPLE_RATE).sin();
        }
        out
    }

    #[test]
    fn bass_tone_lights_bass_band() {
        let b = BandBinner::new();
        let s = sine(80.0, FFT_SIZE);
        let bands = b.process(&s);
        // Bass should dominate
        assert!(bands[0] > bands[1]);
        assert!(bands[0] > bands[2]);
        assert!(bands[0] > bands[3]);
    }

    #[test]
    fn treble_tone_lights_treble_band() {
        let b = BandBinner::new();
        let s = sine(8000.0, FFT_SIZE);
        let bands = b.process(&s);
        assert!(bands[3] > bands[0]);
        assert!(bands[3] > bands[1]);
        assert!(bands[3] > bands[2]);
    }

    #[test]
    fn silence_yields_low_log_mag() {
        let b = BandBinner::new();
        let s = [0.0; FFT_SIZE];
        let bands = b.process(&s);
        for v in bands {
            // Log of near-zero magnitude → very negative
            assert!(v < 0.0);
        }
    }

    #[test]
    fn band_ranges_at_44100_1024() {
        let r = BandRanges::for_44100_1024();
        // Bass: ~bin 1 to ~5 (43-215 Hz)
        assert!(r.bass.0 >= 1);
        assert!(r.bass.1 >= 4 && r.bass.1 <= 6);
        // Treble extends near Nyquist
        assert!(r.treble.1 < FFT_SIZE / 2);
    }
}
