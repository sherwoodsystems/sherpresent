//! Audio input capture for live captions.
//!
//! Captures from a selectable cpal input device, downmixes to mono, resamples
//! to the 16 kHz the caption providers require, and emits fixed 100 ms chunks
//! of 16-bit LE PCM.
//!
//! ## Threading
//!
//! cpal's `Stream` is `!Send` on some platforms, so it cannot live in
//! `AppState` or be moved into a tokio task. Instead one dedicated OS thread
//! owns the stream for its whole lifetime and also runs the resampling loop.
//! The realtime audio callback does only a downmix and a channel send; all
//! resampling happens on that thread but outside the callback.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use serde::Serialize;
use tokio::sync::mpsc::UnboundedSender;

/// Sample rate required by the caption providers.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Providers expect audio in 100 ms chunks.
pub const CHUNK_MS: usize = 100;

/// Samples per emitted chunk (1600 samples = 3200 bytes at 16 kHz mono).
pub const CHUNK_SAMPLES: usize = (TARGET_SAMPLE_RATE as usize / 1000) * CHUNK_MS;

/// An audio input device offered to the user.
#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    pub name: String,
    #[serde(rename = "isDefault")]
    pub is_default: bool,
    #[serde(rename = "sampleRate")]
    pub sample_rate: u32,
    pub channels: u16,
}

/// Enumerate usable audio input devices.
///
/// Devices that fail to report a default input config are skipped rather than
/// failing the whole listing — a machine with one broken device should still
/// show the others.
pub fn list_input_devices() -> Result<Vec<AudioDevice>, String> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();

    let devices = host
        .input_devices()
        .map_err(|e| format!("Failed to enumerate input devices: {}", e))?;

    let mut out = Vec::new();
    for device in devices {
        let name = match device.name() {
            Ok(n) => n,
            Err(e) => {
                log::warn!("Skipping input device with unreadable name: {}", e);
                continue;
            }
        };
        let config = match device.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Skipping input device '{}': {}", name, e);
                continue;
            }
        };
        out.push(AudioDevice {
            is_default: name == default_name,
            name,
            sample_rate: config.sample_rate().0,
            channels: config.channels(),
        });
    }

    Ok(out)
}

/// Handle to a running capture thread.
pub struct AudioCaptureHandle {
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<()>,
    /// Native rate of the device we opened, for logging/diagnostics.
    pub device_sample_rate: u32,
    pub device_name: String,
}

impl AudioCaptureHandle {
    /// Signal the capture thread to stop and wait for it to exit.
    pub fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        if self.thread.join().is_err() {
            log::error!("Audio capture thread panicked");
        }
        log::info!("Audio capture stopped");
    }
}

/// Start capturing from `device_name` (or the system default when `None`).
///
/// Emits `Vec<i16>` chunks of exactly [`CHUNK_SAMPLES`] mono samples at
/// [`TARGET_SAMPLE_RATE`] to `chunk_tx`.
pub fn start(
    device_name: Option<String>,
    chunk_tx: UnboundedSender<Vec<i16>>,
) -> Result<AudioCaptureHandle, String> {
    let host = cpal::default_host();

    let device = match &device_name {
        Some(wanted) => host
            .input_devices()
            .map_err(|e| format!("Failed to enumerate input devices: {}", e))?
            .find(|d| d.name().map(|n| &n == wanted).unwrap_or(false))
            .ok_or_else(|| format!("Audio input device '{}' not found", wanted))?,
        None => host
            .default_input_device()
            .ok_or_else(|| "No default audio input device available".to_string())?,
    };

    let resolved_name = device.name().unwrap_or_else(|_| "<unknown>".to_string());
    let supported = device
        .default_input_config()
        .map_err(|e| format!("Failed to read config for '{}': {}", resolved_name, e))?;

    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let in_rate = config.sample_rate.0;
    let channels = config.channels as usize;

    log::info!(
        "Opening audio input '{}' ({} Hz, {} ch, {:?}) -> {} Hz mono",
        resolved_name,
        in_rate,
        channels,
        sample_format,
        TARGET_SAMPLE_RATE
    );

    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();

    // Used to surface stream-construction failures back to the caller, since
    // the stream must be built on the capture thread.
    let (ready_tx, ready_rx) = std_mpsc::channel::<Result<(), String>>();

    let thread = std::thread::Builder::new()
        .name("sherpresent-audio".into())
        .spawn(move || {
            // Realtime callback -> resample loop. Bounded only by how fast the
            // loop below drains it, which is far faster than realtime.
            let (raw_tx, raw_rx) = std_mpsc::channel::<Vec<f32>>();

            let err_fn = |e| log::error!("Audio input stream error: {}", e);

            let stream = match sample_format {
                cpal::SampleFormat::F32 => {
                    build_stream::<f32>(&device, &config, channels, raw_tx, err_fn)
                }
                cpal::SampleFormat::I16 => {
                    build_stream::<i16>(&device, &config, channels, raw_tx, err_fn)
                }
                cpal::SampleFormat::U16 => {
                    build_stream::<u16>(&device, &config, channels, raw_tx, err_fn)
                }
                cpal::SampleFormat::I32 => {
                    build_stream::<i32>(&device, &config, channels, raw_tx, err_fn)
                }
                cpal::SampleFormat::I8 => {
                    build_stream::<i8>(&device, &config, channels, raw_tx, err_fn)
                }
                cpal::SampleFormat::U8 => {
                    build_stream::<u8>(&device, &config, channels, raw_tx, err_fn)
                }
                other => Err(format!("Unsupported sample format: {:?}", other)),
            };

            let stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };

            if let Err(e) = stream.play() {
                let _ = ready_tx.send(Err(format!("Failed to start audio stream: {}", e)));
                return;
            }

            let _ = ready_tx.send(Ok(()));

            // The stream must stay in scope on this thread for capture to continue.
            run_resample_loop(raw_rx, chunk_tx, in_rate, stop_thread);
            drop(stream);
        })
        .map_err(|e| format!("Failed to spawn audio capture thread: {}", e))?;

    // Wait for the stream to actually open so a bad device is reported to the
    // user immediately rather than as silence.
    match ready_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => Ok(AudioCaptureHandle {
            stop,
            thread,
            device_sample_rate: in_rate,
            device_name: resolved_name,
        }),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            stop.store(true, Ordering::Relaxed);
            Err("Timed out opening the audio input stream".to_string())
        }
    }
}

/// Build an input stream for a concrete sample format, downmixing to mono f32.
fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    raw_tx: std_mpsc::Sender<Vec<f32>>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                // Realtime thread: downmix and hand off, nothing more.
                let frames = data.len() / channels.max(1);
                let mut mono = Vec::with_capacity(frames);
                for frame in data.chunks_exact(channels.max(1)) {
                    let sum: f32 = frame.iter().map(|s| f32::from_sample(*s)).sum();
                    mono.push(sum / channels as f32);
                }
                // A closed receiver means we are shutting down; dropping is correct.
                let _ = raw_tx.send(mono);
            },
            err_fn,
            None,
        )
        .map_err(|e| format!("Failed to build input stream: {}", e))
}

/// Drain mono samples at the device rate, resample to 16 kHz, emit 100 ms chunks.
fn run_resample_loop(
    raw_rx: std_mpsc::Receiver<Vec<f32>>,
    chunk_tx: UnboundedSender<Vec<i16>>,
    in_rate: u32,
    stop: Arc<AtomicBool>,
) {
    let mut resampler = if in_rate == TARGET_SAMPLE_RATE {
        None
    } else {
        match make_resampler(in_rate) {
            Ok(r) => Some(r),
            Err(e) => {
                log::error!("Failed to create resampler ({} Hz): {}", in_rate, e);
                return;
            }
        }
    };

    // Input staged at the device rate, waiting for a full resampler block.
    let mut in_buf: Vec<f32> = Vec::new();
    // Output staged at 16 kHz, waiting for a full 100 ms chunk.
    let mut out_buf: Vec<i16> = Vec::with_capacity(CHUNK_SAMPLES * 2);

    while !stop.load(Ordering::Relaxed) {
        let mono = match raw_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(m) => m,
            Err(std_mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std_mpsc::RecvTimeoutError::Disconnected) => break,
        };

        match resampler.as_mut() {
            None => out_buf.extend(mono.iter().map(|s| to_i16(*s))),
            Some(r) => {
                use rubato::Resampler;
                in_buf.extend_from_slice(&mono);
                loop {
                    let needed = r.input_frames_next();
                    if in_buf.len() < needed {
                        break;
                    }
                    let block: Vec<f32> = in_buf.drain(..needed).collect();
                    match r.process(&[block], None) {
                        Ok(out) => {
                            if let Some(ch) = out.first() {
                                out_buf.extend(ch.iter().map(|s| to_i16(*s)));
                            }
                        }
                        Err(e) => {
                            log::error!("Resampler error: {}", e);
                            return;
                        }
                    }
                }
            }
        }

        while out_buf.len() >= CHUNK_SAMPLES {
            let chunk: Vec<i16> = out_buf.drain(..CHUNK_SAMPLES).collect();
            if chunk_tx.send(chunk).is_err() {
                // Provider side went away.
                log::debug!("Caption chunk receiver closed; stopping capture loop");
                return;
            }
        }
    }
}

/// Bandlimited resampler from `in_rate` down to 16 kHz.
///
/// Sinc (not polynomial) interpolation, because downsampling without an
/// anti-aliasing filter folds 8-24 kHz content back into the speech band and
/// measurably degrades recognition.
fn make_resampler(in_rate: u32) -> Result<rubato::SincFixedIn<f32>, String> {
    use rubato::{SincInterpolationParameters, SincInterpolationType, WindowFunction};

    let window = WindowFunction::BlackmanHarris2;
    let sinc_len = 128;
    let params = SincInterpolationParameters {
        sinc_len,
        f_cutoff: rubato::calculate_cutoff::<f32>(sinc_len, window),
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window,
    };

    // Roughly 20 ms of input per block at typical rates: small enough to keep
    // latency low, large enough that per-block overhead is negligible.
    let chunk_size = (in_rate as usize / 50).max(64);

    rubato::SincFixedIn::<f32>::new(
        TARGET_SAMPLE_RATE as f64 / in_rate as f64,
        1.1,
        params,
        chunk_size,
        1,
    )
    .map_err(|e| e.to_string())
}

/// Clamp and scale a normalized float sample to 16-bit PCM.
#[inline]
fn to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

/// Encode 16-bit samples as little-endian bytes for transmission.
pub fn samples_to_le_bytes(samples: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_samples_is_100ms() {
        assert_eq!(CHUNK_SAMPLES, 1600);
        assert_eq!(samples_to_le_bytes(&vec![0i16; CHUNK_SAMPLES]).len(), 3200);
    }

    #[test]
    fn test_to_i16_clamps() {
        assert_eq!(to_i16(0.0), 0);
        assert_eq!(to_i16(1.0), i16::MAX);
        assert_eq!(to_i16(2.0), i16::MAX);
        assert_eq!(to_i16(-2.0), -i16::MAX);
    }

    #[test]
    fn test_samples_to_le_bytes_is_little_endian() {
        // 0x0102 -> [0x02, 0x01]
        assert_eq!(samples_to_le_bytes(&[0x0102]), vec![0x02, 0x01]);
    }

    #[test]
    fn test_resampler_constructs_for_common_rates() {
        for rate in [44_100u32, 48_000, 96_000] {
            assert!(
                make_resampler(rate).is_ok(),
                "failed to build resampler for {} Hz",
                rate
            );
        }
    }

    #[test]
    fn test_resampler_output_length_ratio() {
        use rubato::Resampler;
        let mut r = make_resampler(48_000).unwrap();

        // The first chunk comes up short by the sinc filter's group delay
        // (~half the 128-tap window, scaled by the ratio) because the filter is
        // still filling. Only steady state tells us the 3:1 ratio is right, so
        // discard the first chunk and measure the ones after it.
        let needed = r.input_frames_next();
        let _warmup = r.process(&[vec![0.0f32; needed]], None).unwrap();

        let mut total_in = 0usize;
        let mut total_out = 0usize;
        for _ in 0..4 {
            let needed = r.input_frames_next();
            let out = r.process(&[vec![0.0f32; needed]], None).unwrap();
            total_in += needed;
            total_out += out[0].len();
        }

        // 48k -> 16k is 3:1. A frame either side covers rounding as the
        // resampler's fractional position advances.
        let expected = total_in / 3;
        assert!(
            total_out.abs_diff(expected) <= 2,
            "expected ~{} output frames over {} input, got {}",
            expected,
            total_in,
            total_out
        );
    }
}
