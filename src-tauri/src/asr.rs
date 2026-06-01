//! ASR HTTP client + audio chunking for the transcribe worker (Phase 2).
//!
//! Two endpoint shapes, keyed on `audio_format`:
//!   - "openai_transcribe" → multipart POST to /audio/transcriptions.
//!   - else (streaming)     → /chat/completions with a base64 audio_url part +
//!     `stream: true`; we accumulate the SSE delta content.
//!
//! Audio is converted to 16kHz mono WAV and split into `chunk_seconds` chunks
//! via one ffmpeg segment call (ports worker/app/audio.py convert_and_split).
//!
//! Token usage is intentionally NOT tracked — the single-user app dropped the
//! admin usage table, so old G-TASK-011 (fault-tolerant token recording) is
//! moot (no recording happens). See decisions.md.

#![allow(dead_code)] // Phase 2: consumed by the transcribe worker.

use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use serde_json::json;

use crate::error::{Error, Result};
use crate::models::AiEndpoint;

const ASR_TIMEOUT: Duration = Duration::from_secs(300);

fn language_hint(language: &str) -> &'static str {
    match language {
        "kor" => " The audio is in Korean. Transcribe it in Korean (Hangul) only.",
        "eng" => " The audio is in English. Transcribe it in English only.",
        _ => "",
    }
}

fn language_iso(language: &str) -> Option<&'static str> {
    match language {
        "kor" => Some("ko"),
        "eng" => Some("en"),
        _ => None,
    }
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(ASR_TIMEOUT)
        .build()
        .map_err(|e| Error::Other(format!("http client build failed: {e}")))
}

fn bearer(endpoint: &AiEndpoint) -> String {
    if endpoint.api_key.is_empty() {
        "dummy".to_string()
    } else {
        endpoint.api_key.clone()
    }
}

/// Convert `src` to 16kHz mono WAV and segment into ~`chunk_seconds` chunks via
/// one ffmpeg call. Returns the chunk paths (sorted) written under `out_dir`.
pub async fn split_to_wav_chunks(
    src: &Path,
    out_dir: &Path,
    chunk_seconds: u32,
) -> Result<Vec<PathBuf>> {
    tokio::fs::create_dir_all(out_dir).await?;
    let pattern = out_dir.join("chunk_%03d.wav");
    let status = crate::ffmpeg::command()
        .args(["-y", "-i"])
        .arg(src)
        .args(["-ar", "16000", "-ac", "1", "-f", "segment", "-segment_time"])
        .arg(chunk_seconds.to_string())
        .arg(&pattern)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map_err(|e| Error::Other(format!("ffmpeg spawn failed: {e}")))?;
    if !status.success() {
        return Err(Error::Other(format!(
            "ffmpeg chunk split exited with {}",
            status.code().unwrap_or(-1)
        )));
    }

    let mut chunks: Vec<PathBuf> = Vec::new();
    let mut entries = tokio::fs::read_dir(out_dir).await?;
    while let Some(e) = entries.next_entry().await? {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) == Some("wav") {
            chunks.push(p);
        }
    }
    chunks.sort();
    Ok(chunks)
}

/// Estimate 16kHz mono 16-bit WAV duration in seconds (44-byte header, 32000
/// bytes/sec). Mirrors audio.py get_wav_duration.
pub fn wav_duration_secs(byte_len: usize) -> f64 {
    byte_len.saturating_sub(44) as f64 / 32000.0
}

/// Transcribe one WAV chunk. `Ok(None)` = endpoint produced no usable output
/// (silent chunk); `Err` = transport/HTTP failure so the caller can retry
/// (G-TASK-008).
pub async fn asr_chunk(
    endpoint: &AiEndpoint,
    request_mode: &str,
    wav: &[u8],
    duration: f64,
    language: &str,
    max_tokens: i64,
) -> Result<Option<String>> {
    // eb0b667 — request_mode: "transcriptions" → multipart /audio/transcriptions;
    // else ("chat_completions") → streaming /chat/completions with audio_url.
    if request_mode == "transcriptions" {
        asr_openai_transcribe(endpoint, wav, language).await
    } else {
        asr_streaming(endpoint, wav, duration, language, max_tokens).await
    }
}

async fn asr_streaming(
    endpoint: &AiEndpoint,
    wav: &[u8],
    duration: f64,
    language: &str,
    max_tokens: i64,
) -> Result<Option<String>> {
    let url = format!(
        "{}/chat/completions",
        endpoint.api_base_url.trim_end_matches('/')
    );
    let audio_b64 = base64::engine::general_purpose::STANDARD.encode(wav);
    let data_url = format!("data:audio/wav;base64,{audio_b64}");
    let mut system = String::from(
        "You are a helpful assistant that transcribes audio input into text output in JSON format.",
    );
    system.push_str(language_hint(language));
    let prompt_text = format!(
        "This is a {duration:.2} seconds audio, please transcribe it with these keys: Start time, End time, Speaker ID, Content"
    );
    let payload = json!({
        "model": endpoint.model_id,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": [
                {"type": "audio_url", "audio_url": {"url": data_url}},
                {"type": "text", "text": prompt_text},
            ]},
        ],
        "max_tokens": max_tokens,
        "stream": true,
    });

    let resp = client()?
        .post(&url)
        .bearer_auth(bearer(endpoint))
        .json(&payload)
        .send()
        .await
        .map_err(|e| Error::Other(format!("asr request error: {e}")))?;
    if !resp.status().is_success() {
        return Err(Error::Other(format!("asr http {}", resp.status())));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| Error::Other(format!("asr body read error: {e}")))?;

    // Accumulate SSE `data:` delta content.
    let mut out = String::new();
    for line in body.lines() {
        let Some(data) = line.trim_start().strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
            break;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
            if let Some(c) = v["choices"][0]["delta"]["content"].as_str() {
                out.push_str(c);
            }
        }
    }
    let out = out.trim().to_string();
    Ok(if out.is_empty() { None } else { Some(out) })
}

async fn asr_openai_transcribe(
    endpoint: &AiEndpoint,
    wav: &[u8],
    language: &str,
) -> Result<Option<String>> {
    let url = format!(
        "{}/audio/transcriptions",
        endpoint.api_base_url.trim_end_matches('/')
    );
    let mut form = reqwest::multipart::Form::new()
        .text("model", endpoint.model_id.clone())
        .text("response_format", "json")
        .part(
            "file",
            reqwest::multipart::Part::bytes(wav.to_vec())
                .file_name("audio.wav")
                .mime_str("audio/wav")
                .map_err(|e| Error::Other(format!("multipart mime: {e}")))?,
        );
    if let Some(iso) = language_iso(language) {
        form = form.text("language", iso);
    }

    let resp = client()?
        .post(&url)
        .bearer_auth(bearer(endpoint))
        .multipart(form)
        .send()
        .await
        .map_err(|e| Error::Other(format!("asr request error: {e}")))?;
    if !resp.status().is_success() {
        return Err(Error::Other(format!("asr http {}", resp.status())));
    }
    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| Error::Other(format!("asr json error: {e}")))?;
    let text = v["text"].as_str().unwrap_or_default().trim().to_string();
    Ok(if text.is_empty() { None } else { Some(text) })
}
