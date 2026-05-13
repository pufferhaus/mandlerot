//! Audio worker thread. Pulls 1024-sample windows from the capture ring at
//! ~100 Hz, runs FFT + binning + envelopes + auto-gain + beat detection, and
//! writes the smoothed/normalized 4 band values + beat trigger to atomics
//! visible to the render thread.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::audio::bands::{BandBinner, BAND_COUNT, FFT_SIZE};
use crate::audio::beat::BeatDetector;
use crate::audio::capture::CaptureStream;
use crate::audio::envelope::{AutoGain, EnvelopeFollower};
use crate::audio::params::AudioParams;

const UPDATE_HZ: f32 = 100.0;

/// Lock-free shared audio state. f32 stored bit-cast as u32.
pub struct AtomicAudio {
    bands: [AtomicU32; BAND_COUNT],
    beat: AtomicU32,
    /// True when the capture device is open and producing samples. Goes false
    /// if `CaptureStream::open_default` fails or if no samples arrive within
    /// the startup grace window. Main reads this once at startup to decide
    /// whether to default `audio_bypass=true`.
    present: AtomicBool,
    /// Goes true once the audio thread has finished probing for input — i.e.
    /// either samples have arrived, or the grace window expired without any.
    /// Main waits for this to flip before sampling `present` to decide the
    /// initial bypass state.
    probed: AtomicBool,
}

impl AtomicAudio {
    pub fn new() -> Self {
        Self {
            bands: std::array::from_fn(|_| AtomicU32::new(0)),
            beat: AtomicU32::new(0),
            present: AtomicBool::new(true),
            probed: AtomicBool::new(false),
        }
    }

    pub fn store_bands(&self, bands: [f32; BAND_COUNT]) {
        for (a, v) in self.bands.iter().zip(bands.iter()) {
            a.store(v.to_bits(), Ordering::Relaxed);
        }
    }

    pub fn store_beat(&self, v: f32) {
        self.beat.store(v.to_bits(), Ordering::Relaxed);
    }

    pub fn load_bands(&self) -> [f32; BAND_COUNT] {
        std::array::from_fn(|i| f32::from_bits(self.bands[i].load(Ordering::Relaxed)))
    }

    pub fn load_beat(&self) -> f32 {
        f32::from_bits(self.beat.load(Ordering::Relaxed))
    }

    pub fn set_present(&self, v: bool) {
        self.present.store(v, Ordering::Relaxed);
    }

    pub fn is_present(&self) -> bool {
        self.present.load(Ordering::Relaxed)
    }

    pub fn mark_probed(&self) {
        self.probed.store(true, Ordering::Relaxed);
    }

    pub fn is_probed(&self) -> bool {
        self.probed.load(Ordering::Relaxed)
    }
}

impl Default for AtomicAudio {
    fn default() -> Self {
        Self::new()
    }
}

pub fn spawn(
    atomic: Arc<AtomicAudio>,
    params: Arc<AudioParams>,
    stop: Arc<std::sync::atomic::AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let stream = match CaptureStream::open_default() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("audio: {e}; running without audio reactivity");
                atomic.set_present(false);
                atomic.mark_probed();
                return;
            }
        };
        let binner = BandBinner::new();
        let mut envs: [EnvelopeFollower; BAND_COUNT] =
            std::array::from_fn(|_| EnvelopeFollower::new(0.005, 0.2, UPDATE_HZ));
        // Initial noise floor seeds the AutoGain; it's refreshed each tick
        // from the live `params` Arc so the Settings menu can tune it
        // without restart. Env var still respected for one-shot overrides
        // (CI, smoke tests) but is no longer the only knob.
        let initial_floor = std::env::var("MANDLEROT_NOISE_FLOOR")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| params.noise_floor());
        let mut gains: [AutoGain; BAND_COUNT] =
            std::array::from_fn(|_| AutoGain::new(5.0, UPDATE_HZ, initial_floor));
        let mut beat = BeatDetector::new(UPDATE_HZ);
        let mut window = [0.0; FFT_SIZE];
        let dt = Duration::from_secs_f32(1.0 / UPDATE_HZ);
        // 1 second grace window. If no samples arrive in that time, treat the
        // device as absent so the main loop can default `audio_bypass=true`.
        let mut probe_ticks: u32 = 0;
        const PROBE_TICKS_MAX: u32 = UPDATE_HZ as u32;

        while !stop.load(Ordering::Relaxed) {
            let got = {
                let rb = stream.ring.lock().unwrap();
                rb.read_latest(&mut window)
            };
            if !atomic.is_probed() {
                if got {
                    atomic.mark_probed();
                } else {
                    probe_ticks += 1;
                    if probe_ticks >= PROBE_TICKS_MAX {
                        atomic.set_present(false);
                        atomic.mark_probed();
                    }
                }
            }
            if got {
                // Refresh live-tunable params on each tick. Cheap: 5 atomic
                // loads + arithmetic. The cost of a stale value for one tick
                // (worst case) is invisible to the eye.
                let floor = params.noise_floor();
                let (raw_bands, spec_mags) = binner.process_with_mags(&window);
                let mut out = [0.0; BAND_COUNT];
                for i in 0..BAND_COUNT {
                    let lin = raw_bands[i].exp().min(1e6); // log → linear
                    envs[i].update(lin);
                    gains[i].set_min_reference(floor);
                    gains[i].observe(envs[i].value);
                    let normalized = gains[i].normalize(envs[i].value, 50);
                    out[i] = (normalized * params.gain(i)).clamp(0.0, 1.0);
                }
                atomic.store_bands(out);
                // Beat detection on the same window's spectral magnitudes
                // (linear, per-bin) — NOT on time-domain PCM samples.
                beat.update(&spec_mags);
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
        a.store_bands([0.1, 0.2, 0.3, 0.4, 0.5]);
        let got = a.load_bands();
        assert_eq!(got, [0.1, 0.2, 0.3, 0.4, 0.5]);
    }

    #[test]
    fn atomic_beat_roundtrips() {
        let a = AtomicAudio::new();
        a.store_beat(0.75);
        assert_eq!(a.load_beat(), 0.75);
    }
}
