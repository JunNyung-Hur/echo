//! Native audio capture via cpal (D-023).
//!
//! cpal 0.16 exposes WASAPI loopback as an *output* endpoint opened as a capture
//! stream, so we report output endpoints as selectable "system" sources and mic
//! inputs as "mic" — one unified device list for the UI. Recording sessions
//! write 5s WAV chunks on a dedicated capture thread (see below).

use std::f32::consts::PI;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
pub struct AudioDeviceInfo {
    /// OS-reported device name.
    pub name: String,
    /// "mic" (real input) or "system" (output endpoint captured via loopback).
    pub source: String,
    pub is_default: bool,
    /// Can cpal open a capture stream from this device?
    pub capturable: bool,
}

/// Enumerate selectable capture sources: real mic inputs + output endpoints we
/// can loopback-capture. This is the single unified list the UI will show.
pub fn enumerate() -> Vec<AudioDeviceInfo> {
    let host = cpal::default_host();
    let mut out: Vec<AudioDeviceInfo> = Vec::new();

    let default_in = host.default_input_device().and_then(|d| d.name().ok());
    let default_out = host.default_output_device().and_then(|d| d.name().ok());

    if let Ok(devices) = host.input_devices() {
        for d in devices {
            let name = d.name().unwrap_or_else(|_| "(unknown)".into());
            let capturable = d.default_input_config().is_ok();
            out.push(AudioDeviceInfo {
                is_default: Some(&name) == default_in.as_ref(),
                name,
                source: "mic".into(),
                capturable,
            });
        }
    }

    // Output endpoints → loopback capture candidates (cpal 0.16 WASAPI loopback).
    if let Ok(devices) = host.output_devices() {
        for d in devices {
            let name = d.name().unwrap_or_else(|_| "(unknown)".into());
            let capturable = d.default_output_config().is_ok();
            out.push(AudioDeviceInfo {
                is_default: Some(&name) == default_out.as_ref(),
                name,
                source: "system".into(),
                capturable,
            });
        }
    }

    out
}

// ============================================================================
// Recording session (P1R-01) — native cpal capture → 5s WAV chunks
// ============================================================================
//
// cpal `Stream` is `!Send`, so the stream lives on a dedicated capture thread.
// The audio callback pushes interleaved f32 samples into a shared buffer; the
// same thread drains it every ~50ms to compute the RMS level (emitted as a
// Tauri event for the waveform) and flushes a WAV chunk every 5s.
//
// Guards (re-implemented from the browser pipeline — [[feedback-preserve-all-guards]]):
//   - G-REC-001  5s WAV chunk + monotonic seq owned by this thread
//   - G-REC-002  last_chunk_at touched on every flush (heartbeat)
//   - G-REC-003  drain-then-write; samples never written twice
//   - G-REC-004  flush failure counter → "recording:chunk_error" event after 3
//   - silence detection → "recording:level" carries rms; UI decides warnings

const CHUNK_SECONDS: u64 = 5;
/// Spectrum bars the capture thread emits; the UI resamples to its own count.
const WAVE_BARS: usize = 80;
/// FFT window for the waveform spectrum (≈85ms @48kHz).
const FFT_SIZE: usize = 4096;
/// Linear gain before sqrt in spectrum normalization (tune for bar fullness).
const SPECTRUM_GAIN: f32 = 18.0;

#[derive(Clone, Serialize)]
struct WavePayload {
    recording_id: String,
    /// Per-bar amplitude (0..1) for this frame — a snapshot of the current
    /// audio, drawn as a centre-symmetric waveform by the UI.
    bars: Vec<f32>,
}

/// FFT magnitude spectrum of the recent mono window → `bins` bars over the
/// speech range (~0–750Hz+), EMA-smoothed across frames. Mirrors the old
/// browser AnalyserNode `getByteFrequencyData` visualization (energy on the
/// left, tapering right — not a uniform time-domain block).
fn compute_spectrum(
    ring: &[f32],
    fft: &dyn Fft<f32>,
    hann: &[f32],
    sample_rate: u32,
    bins: usize,
    smoothed: &mut [f32],
) -> Vec<f32> {
    let n = FFT_SIZE;
    let mut buf = vec![Complex::<f32>::new(0.0, 0.0); n];
    // Newest n samples, Hann-windowed; zero-padded if the ring is shorter.
    let start = ring.len().saturating_sub(n);
    for (i, &s) in ring[start..].iter().enumerate() {
        buf[i].re = s * hann[i];
    }
    fft.process(&mut buf);

    // Speech range ~0–750Hz (old SPEECH_BIN_LIMIT analogue); widened to at
    // least `bins` so each bar maps to a distinct bin.
    let max_bin = ((750.0 * n as f32 / sample_rate as f32).ceil() as usize).clamp(bins, n / 2);
    let mut out = vec![0.0f32; bins];
    for (b, slot) in out.iter_mut().enumerate() {
        let bin = (b * max_bin / bins).min(n / 2 - 1);
        let mag = buf[bin].norm();
        let v = ((mag / n as f32) * SPECTRUM_GAIN).sqrt().min(1.0);
        // EMA across frames — analyser smoothingTimeConstant feel.
        smoothed[b] = smoothed[b] * 0.7 + v * 0.3;
        *slot = smoothed[b];
    }
    out
}

#[derive(Clone, Serialize)]
struct ChunkPayload {
    recording_id: String,
    seq: u32,
}

#[derive(Clone, Serialize)]
struct ChunkErrorPayload {
    recording_id: String,
    message: String,
}

/// Handle to a live capture. Dropping or calling `stop()` ends the thread and
/// flushes the final partial chunk.
pub struct CaptureHandle {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl CaptureHandle {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Start a capture session. Resolves the device + opens the stream on a
/// dedicated thread; returns once the stream is confirmed playing (or errors).
///
/// `chunk_dir` must already exist. Chunks are written as `<seq:06d>.wav`.
/// `on_flush` is called (off the capture thread is NOT guaranteed — it runs ON
/// the capture thread) after each chunk so the caller can touch last_chunk_at.
pub fn start_capture(
    app: AppHandle,
    device_name: String,
    source: String,
    chunk_dir: PathBuf,
    recording_id: String,
    on_flush: impl Fn(u32) + Send + 'static,
) -> Result<CaptureHandle, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();

    let (init_tx, init_rx) = std::sync::mpsc::channel::<Result<(u32, u16), String>>();

    let thread = std::thread::Builder::new()
        .name(format!("capture-{recording_id}"))
        .spawn(move || {
            capture_thread_main(
                app,
                device_name,
                source,
                chunk_dir,
                recording_id,
                stop_thread,
                init_tx,
                on_flush,
            );
        })
        .map_err(|e| format!("capture thread spawn 실패: {e}"))?;

    // Wait for the stream to confirm it opened (or report the error).
    match init_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok((sample_rate, channels))) => Ok(CaptureHandle {
            stop,
            thread: Some(thread),
            sample_rate,
            channels,
        }),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            stop.store(true, Ordering::SeqCst);
            let _ = thread.join();
            Err("캡처 스트림 초기화 시간 초과".into())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn capture_thread_main(
    app: AppHandle,
    device_name: String,
    source: String,
    chunk_dir: PathBuf,
    recording_id: String,
    stop: Arc<AtomicBool>,
    init_tx: std::sync::mpsc::Sender<Result<(u32, u16), String>>,
    on_flush: impl Fn(u32),
) {
    let host = cpal::default_host();
    let device = if source == "system" {
        host.output_devices()
            .ok()
            .and_then(|mut it| it.find(|d| d.name().ok().as_deref() == Some(&device_name)))
    } else {
        host.input_devices()
            .ok()
            .and_then(|mut it| it.find(|d| d.name().ok().as_deref() == Some(&device_name)))
    };
    let device = match device {
        Some(d) => d,
        None => {
            let _ = init_tx.send(Err(format!("장치를 찾을 수 없음: {device_name}")));
            return;
        }
    };

    let config = if source == "system" {
        device.default_output_config()
    } else {
        device.default_input_config()
    };
    let config = match config {
        Ok(c) => c,
        Err(e) => {
            let _ = init_tx.send(Err(format!("config 조회 실패: {e}")));
            return;
        }
    };

    let sample_rate = config.sample_rate().0;
    let channels = config.channels();
    let sample_format = config.sample_format();
    let stream_config: cpal::StreamConfig = config.into();

    // Shared interleaved f32 buffer filled by the audio callback.
    let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let cb_buffer = buffer.clone();
    let err_fn = |e| tracing::warn!(?e, "capture stream error");

    let stream_result: Result<cpal::Stream, cpal::BuildStreamError> = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            &stream_config,
            move |data: &[f32], _| {
                if let Ok(mut b) = cb_buffer.lock() {
                    b.extend_from_slice(data);
                }
            },
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &stream_config,
            move |data: &[i16], _| {
                if let Ok(mut b) = cb_buffer.lock() {
                    b.extend(data.iter().map(|&s| s as f32 / i16::MAX as f32));
                }
            },
            err_fn,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            &stream_config,
            move |data: &[u16], _| {
                if let Ok(mut b) = cb_buffer.lock() {
                    b.extend(
                        data.iter()
                            .map(|&s| (s as f32 / u16::MAX as f32) * 2.0 - 1.0),
                    );
                }
            },
            err_fn,
            None,
        ),
        other => {
            let _ = init_tx.send(Err(format!("지원하지 않는 샘플 포맷: {other:?}")));
            return;
        }
    };

    let stream = match stream_result {
        Ok(s) => s,
        Err(e) => {
            let _ = init_tx.send(Err(format!("스트림 열기 실패: {e}")));
            return;
        }
    };
    if let Err(e) = stream.play() {
        let _ = init_tx.send(Err(format!("스트림 재생 실패: {e}")));
        return;
    }

    // Stream is live — unblock the caller.
    let _ = init_tx.send(Ok((sample_rate, channels)));
    tracing::info!(%recording_id, sample_rate, channels, %source, "capture started");

    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let mut chunk_acc: Vec<f32> = Vec::new();
    let mut seq: u32 = 0;
    let mut last_flush = Instant::now();
    let mut consecutive_fails: u32 = 0;

    // Waveform: rolling mono ring → FFT magnitude spectrum (mirrors the old
    // browser AnalyserNode look). Kept separate from chunk_acc so the audio
    // capture path (G-REC-001/003) is untouched.
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    let hann: Vec<f32> = (0..FFT_SIZE)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / (FFT_SIZE as f32 - 1.0)).cos())
        .collect();
    let mut mono_ring: Vec<f32> = Vec::with_capacity(FFT_SIZE * 2);
    let mut smoothed: Vec<f32> = vec![0.0; WAVE_BARS];

    loop {
        std::thread::sleep(Duration::from_millis(50));

        // Drain whatever the callback accumulated (~50ms of audio).
        let drained: Vec<f32> = {
            match buffer.lock() {
                Ok(mut b) => b.drain(..).collect(),
                Err(_) => Vec::new(),
            }
        };

        if !drained.is_empty() {
            // Feed a downmixed mono copy into the FFT ring (visualization only).
            let ch = channels.max(1) as usize;
            for frame in drained.chunks(ch) {
                mono_ring.push(frame.iter().sum::<f32>() / ch as f32);
            }
            if mono_ring.len() > FFT_SIZE {
                mono_ring.drain(0..mono_ring.len() - FFT_SIZE);
            }
            let bars = compute_spectrum(
                &mono_ring,
                &*fft,
                &hann,
                sample_rate,
                WAVE_BARS,
                &mut smoothed,
            );
            let _ = app.emit(
                "recording:level",
                WavePayload {
                    recording_id: recording_id.clone(),
                    bars,
                },
            );
            // chunk accumulation unchanged (G-REC-003 drain-then-write).
            chunk_acc.extend(drained);
        }

        let should_stop = stop.load(Ordering::SeqCst);

        // G-REC-001: flush a chunk every CHUNK_SECONDS, or on stop.
        if (last_flush.elapsed().as_secs() >= CHUNK_SECONDS || should_stop) && !chunk_acc.is_empty()
        {
            let path = chunk_dir.join(format!("{seq:06}.wav"));
            match write_wav_chunk(&path, spec, &chunk_acc) {
                Ok(()) => {
                    consecutive_fails = 0;
                    on_flush(seq); // G-REC-002 heartbeat (caller touches last_chunk_at)
                    let _ = app.emit(
                        "recording:chunk",
                        ChunkPayload {
                            recording_id: recording_id.clone(),
                            seq,
                        },
                    );
                    seq += 1;
                    chunk_acc.clear();
                    last_flush = Instant::now();
                }
                Err(e) => {
                    consecutive_fails += 1;
                    tracing::warn!(%recording_id, seq, error = %e, "chunk write failed");
                    // G-REC-004: surface after 3 consecutive failures.
                    if consecutive_fails >= 3 {
                        let _ = app.emit(
                            "recording:chunk_error",
                            ChunkErrorPayload {
                                recording_id: recording_id.clone(),
                                message: e,
                            },
                        );
                    }
                    // keep chunk_acc so the next attempt retries the same audio
                    last_flush = Instant::now();
                }
            }
        }

        if should_stop {
            break;
        }
    }

    drop(stream);
    tracing::info!(%recording_id, chunks = seq, "capture stopped");
}

fn write_wav_chunk(path: &PathBuf, spec: hound::WavSpec, samples: &[f32]) -> Result<(), String> {
    let mut writer = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for &s in samples {
        writer.write_sample(s).map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())?;
    Ok(())
}
