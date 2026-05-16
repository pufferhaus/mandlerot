//! cpal capture stream into a lock-free ring buffer of f32 mono samples.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};

use crate::error::{Error, Result};

pub struct CaptureStream {
    _stream: cpal::Stream,
    pub ring: Arc<Mutex<RingBuffer>>,
}

pub struct RingBuffer {
    pub data: Vec<f32>,
    pub head: usize, // next write index
    pub filled: usize,
    pub capacity: usize,
}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: vec![0.0; capacity],
            head: 0,
            filled: 0,
            capacity,
        }
    }

    pub fn push_slice(&mut self, src: &[f32]) {
        for s in src {
            self.data[self.head] = *s;
            self.head = (self.head + 1) % self.capacity;
            self.filled = (self.filled + 1).min(self.capacity);
        }
    }

    /// Read the most recent `n` samples into `dst` (length n). Returns false
    /// if not enough data filled yet.
    pub fn read_latest(&self, dst: &mut [f32]) -> bool {
        let n = dst.len();
        if self.filled < n {
            return false;
        }
        let start = (self.head + self.capacity - n) % self.capacity;
        for (i, slot) in dst.iter_mut().enumerate() {
            *slot = self.data[(start + i) % self.capacity];
        }
        true
    }
}

impl CaptureStream {
    pub fn open_default() -> Result<Self> {
        Self::open_with_device(None)
    }

    /// Open a capture stream with an optional explicit device-name substring.
    /// Resolution order:
    ///   1. `name` if `Some` and non-empty.
    ///   2. `MANDLEROT_AUDIO_DEVICE` env var if set.
    ///   3. Host default input device.
    pub fn open_with_device(name: Option<&str>) -> Result<Self> {
        let host = cpal::default_host();
        let want_name = name
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("MANDLEROT_AUDIO_DEVICE").ok());
        let device = Self::pick_input_device(&host, want_name.as_deref())?;
        let device_name = device.name().unwrap_or_else(|_| "<unknown>".into());
        let supported = device
            .default_input_config()
            .map_err(|e| Error::Backend(format!("default input config: {e}")))?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();
        tracing::info!(device = %device_name, ?config, "opening audio capture");

        let ring = Arc::new(Mutex::new(RingBuffer::new(8192)));
        let ring_for_cb = ring.clone();
        let channels = config.channels as usize;

        // Generic push: optionally down-mix `data` from `channels` interleaved
        // channels to mono, then write into the ring as f32.
        fn push_mono_f32(rb: &mut RingBuffer, data: &[f32], channels: usize) {
            if channels == 1 {
                rb.push_slice(data);
            } else {
                let mut mono = Vec::with_capacity(data.len() / channels);
                for frame in data.chunks_exact(channels) {
                    let avg: f32 = frame.iter().sum::<f32>() / channels as f32;
                    mono.push(avg);
                }
                rb.push_slice(&mono);
            }
        }
        fn log_stream_err(err: cpal::StreamError) {
            tracing::warn!("audio stream error: {err}");
        }
        let stream = match sample_format {
            SampleFormat::F32 => {
                let ring_cb = ring_for_cb.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        let mut rb = ring_cb.lock().unwrap();
                        push_mono_f32(&mut rb, data, channels);
                    },
                    log_stream_err,
                    None,
                )
            }
            SampleFormat::I16 => {
                let ring_cb = ring_for_cb.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let mut rb = ring_cb.lock().unwrap();
                        let f: Vec<f32> =
                            data.iter().map(|s| *s as f32 / 32768.0).collect();
                        push_mono_f32(&mut rb, &f, channels);
                    },
                    log_stream_err,
                    None,
                )
            }
            SampleFormat::U16 => {
                let ring_cb = ring_for_cb.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        let mut rb = ring_cb.lock().unwrap();
                        let f: Vec<f32> = data
                            .iter()
                            .map(|s| (*s as f32 - 32768.0) / 32768.0)
                            .collect();
                        push_mono_f32(&mut rb, &f, channels);
                    },
                    log_stream_err,
                    None,
                )
            }
            other => {
                return Err(Error::Backend(format!(
                    "unsupported sample format: {other:?} (F32/I16/U16 supported)"
                )));
            }
        }
        .map_err(|e| Error::Backend(format!("build input stream: {e}")))?;

        stream
            .play()
            .map_err(|e| Error::Backend(format!("play stream: {e}")))?;

        Ok(Self {
            _stream: stream,
            ring,
        })
    }

    /// Pick an input device. Order:
    ///   1. If `want_substr` set, first device whose name contains it.
    ///   2. Default input device that successfully reports a config.
    ///   3. First enumerated input device that reports a config.
    /// Logs the full input device list so misbehaving setups are diagnosable.
    fn pick_input_device(host: &cpal::Host, want_substr: Option<&str>) -> Result<cpal::Device> {
        let devices: Vec<cpal::Device> = host
            .input_devices()
            .map(|it| it.collect())
            .unwrap_or_default();
        let names: Vec<String> = devices
            .iter()
            .map(|d| d.name().unwrap_or_else(|_| "<unknown>".into()))
            .collect();
        tracing::info!(inputs = ?names, "audio input devices");

        if let Some(want) = want_substr {
            for d in &devices {
                let n = d.name().unwrap_or_default();
                if n.to_lowercase().contains(&want.to_lowercase()) {
                    tracing::info!(picked = %n, by = "MANDLEROT_AUDIO_DEVICE", "audio device");
                    return Ok(d.clone());
                }
            }
            tracing::warn!(want, "MANDLEROT_AUDIO_DEVICE not matched; falling back");
        }

        if let Some(d) = host.default_input_device() {
            if d.default_input_config().is_ok() {
                let n = d.name().unwrap_or_default();
                tracing::info!(picked = %n, by = "default", "audio device");
                return Ok(d);
            }
            tracing::warn!("default input device has no usable config; trying enumeration");
        }

        for d in devices {
            if d.default_input_config().is_ok() {
                let n = d.name().unwrap_or_default();
                tracing::info!(picked = %n, by = "enumeration", "audio device");
                return Ok(d);
            }
        }

        Err(Error::Backend("no usable audio input device".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_pushes_and_reads_latest() {
        let mut rb = RingBuffer::new(8);
        rb.push_slice(&[1.0, 2.0, 3.0, 4.0]);
        let mut out = [0.0; 4];
        assert!(rb.read_latest(&mut out));
        assert_eq!(out, [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn ring_returns_false_when_empty() {
        let rb = RingBuffer::new(8);
        let mut out = [0.0; 4];
        assert!(!rb.read_latest(&mut out));
    }

    #[test]
    fn ring_wraps_around() {
        let mut rb = RingBuffer::new(4);
        rb.push_slice(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let mut out = [0.0; 4];
        assert!(rb.read_latest(&mut out));
        assert_eq!(out, [3.0, 4.0, 5.0, 6.0]);
    }
}
