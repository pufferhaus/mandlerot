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
        for i in 0..n {
            dst[i] = self.data[(start + i) % self.capacity];
        }
        true
    }
}

impl CaptureStream {
    pub fn open_default() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| Error::Backend("no default audio input device".into()))?;
        let supported = device
            .default_input_config()
            .map_err(|e| Error::Backend(format!("default input config: {e}")))?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();
        tracing::info!(?config, "opening audio capture");

        let ring = Arc::new(Mutex::new(RingBuffer::new(8192)));
        let ring_for_cb = ring.clone();
        let channels = config.channels as usize;

        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                &config,
                move |data: &[f32], _info: &cpal::InputCallbackInfo| {
                    let mut rb = ring_for_cb.lock().unwrap();
                    if channels == 1 {
                        rb.push_slice(data);
                    } else {
                        // Down-mix to mono.
                        let mut mono = Vec::with_capacity(data.len() / channels);
                        for frame in data.chunks_exact(channels) {
                            let avg: f32 = frame.iter().sum::<f32>() / channels as f32;
                            mono.push(avg);
                        }
                        rb.push_slice(&mono);
                    }
                },
                |err| tracing::warn!("audio stream error: {err}"),
                None,
            ),
            other => {
                return Err(Error::Backend(format!(
                    "unsupported sample format: {other:?} — only F32 is wired"
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
