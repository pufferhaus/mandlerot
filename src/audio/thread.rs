//! Audio worker thread. Pulls 1024-sample windows from the capture ring at
//! ~100 Hz, runs FFT + binning + envelopes + auto-gain + beat detection, and
//! writes the smoothed/normalized 4 band values + beat trigger to atomics
//! visible to the render thread.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::audio::bands::{BandBinner, FFT_SIZE};
use crate::audio::beat::BeatDetector;
use crate::audio::capture::CaptureStream;
use crate::audio::envelope::{AutoGain, EnvelopeFollower};

const UPDATE_HZ: f32 = 100.0;

/// Lock-free shared audio state. f32 stored bit-cast as u32.
pub struct AtomicAudio {
    bands: [AtomicU32; 4],
    beat: AtomicU32,
}

impl AtomicAudio {
    pub fn new() -> Self {
        Self {
            bands: [
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
            ],
            beat: AtomicU32::new(0),
        }
    }

    pub fn store_bands(&self, bands: [f32; 4]) {
        for (a, v) in self.bands.iter().zip(bands.iter()) {
            a.store(v.to_bits(), Ordering::Relaxed);
        }
    }

    pub fn store_beat(&self, v: f32) {
        self.beat.store(v.to_bits(), Ordering::Relaxed);
    }

    pub fn load_bands(&self) -> [f32; 4] {
        [
            f32::from_bits(self.bands[0].load(Ordering::Relaxed)),
            f32::from_bits(self.bands[1].load(Ordering::Relaxed)),
            f32::from_bits(self.bands[2].load(Ordering::Relaxed)),
            f32::from_bits(self.bands[3].load(Ordering::Relaxed)),
        ]
    }

    pub fn load_beat(&self) -> f32 {
        f32::from_bits(self.beat.load(Ordering::Relaxed))
    }
}

pub fn spawn(
    atomic: Arc<AtomicAudio>,
    stop: Arc<std::sync::atomic::AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let stream = match CaptureStream::open_default() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("audio: {e}; running without audio reactivity");
                return;
            }
        };
        let binner = BandBinner::new();
        let mut envs = [
            EnvelopeFollower::new(0.005, 0.2, UPDATE_HZ),
            EnvelopeFollower::new(0.005, 0.2, UPDATE_HZ),
            EnvelopeFollower::new(0.005, 0.2, UPDATE_HZ),
            EnvelopeFollower::new(0.005, 0.2, UPDATE_HZ),
        ];
        let mut gains = [
            AutoGain::new(5.0, UPDATE_HZ),
            AutoGain::new(5.0, UPDATE_HZ),
            AutoGain::new(5.0, UPDATE_HZ),
            AutoGain::new(5.0, UPDATE_HZ),
        ];
        let mut beat = BeatDetector::new(UPDATE_HZ);
        let mut window = [0.0; FFT_SIZE];
        let dt = Duration::from_secs_f32(1.0 / UPDATE_HZ);

        while !stop.load(Ordering::Relaxed) {
            let got = {
                let rb = stream.ring.lock().unwrap();
                rb.read_latest(&mut window)
            };
            if got {
                let raw_bands = binner.process(&window);
                // Convert log magnitudes to linear-ish, then envelope+gain.
                let mut out = [0.0; 4];
                for i in 0..4 {
                    let lin = raw_bands[i].exp().min(1e6); // log → linear
                    envs[i].update(lin);
                    gains[i].observe(envs[i].value);
                    out[i] = gains[i].normalize(envs[i].value, 50);
                }
                atomic.store_bands(out);
                // Beat detection on the same window's mags.
                // Re-derive linear mags for beat (sum-of-positive-flux):
                let mags: Vec<f32> = window.iter().take(FFT_SIZE / 2).map(|x| x.abs()).collect();
                beat.update(&mags);
                atomic.store_beat(beat.trigger);
            }
            std::thread::sleep(dt);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_audio_roundtrips() {
        let a = AtomicAudio::new();
        a.store_bands([0.1, 0.2, 0.3, 0.4]);
        let got = a.load_bands();
        assert_eq!(got, [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn atomic_beat_roundtrips() {
        let a = AtomicAudio::new();
        a.store_beat(0.75);
        assert_eq!(a.load_beat(), 0.75);
    }
}
