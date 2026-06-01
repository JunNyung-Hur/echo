//! Audio device + input-test commands (D-023).

use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::audio_capture::{self, AudioDeviceInfo};
use crate::error::{Error, Result};
use crate::{ffmpeg, AppState};

/// Unified capture source list (mic inputs + loopback outputs).
#[tauri::command]
pub fn list_audio_devices() -> Result<Vec<AudioDeviceInfo>> {
    Ok(audio_capture::enumerate())
}

/// Get the selected source's OS volume as a 0.0–1.0 scalar (Windows Core Audio).
/// `source` is "mic" or "system" — see [`crate::audio_volume`].
#[tauri::command]
pub fn get_source_volume(name: String, source: String) -> Result<f32> {
    crate::audio_volume::get_volume(&name, &source).map_err(Error::Other)
}

/// Set the selected source's OS volume from a 0.0–1.0 scalar (Windows Core Audio).
#[tauri::command]
pub fn set_source_volume(name: String, source: String, level: f32) -> Result<()> {
    crate::audio_volume::set_volume(&name, &source, level).map_err(Error::Other)
}

// ============================================================================
// Input test (monitor + record-then-playback)
// ============================================================================

/// Start a test capture from the given source. Emits live `recording:level`
/// waveform events keyed by the returned `test_id`. Audio is written to a temp
/// dir; `stop_test_capture` concatenates it and returns the webm bytes so the
/// UI can play it back.
#[tauri::command]
pub async fn start_test_capture(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    source: String,
) -> Result<String> {
    let test_id = format!("test-{}", Uuid::new_v4());
    let dir = test_dir(&app, &test_id)?;
    tokio::fs::create_dir_all(&dir).await?;

    let handle = audio_capture::start_capture(
        app.clone(),
        name,
        source,
        dir,
        test_id.clone(),
        |_seq| {}, // no DB heartbeat for tests
    )
    .map_err(Error::Other)?;

    if let Ok(mut map) = state.captures.lock() {
        map.insert(test_id.clone(), handle);
    }
    Ok(test_id)
}

/// Stop a test capture and return the recorded audio as webm bytes for
/// playback. Cleans up the temp dir.
#[tauri::command]
pub async fn stop_test_capture(
    app: AppHandle,
    state: State<'_, AppState>,
    test_id: String,
) -> Result<Vec<u8>> {
    let handle = {
        match state.captures.lock() {
            Ok(mut map) => map.remove(&test_id),
            Err(_) => None,
        }
    };
    if let Some(h) = handle {
        let _ = tauri::async_runtime::spawn_blocking(move || h.stop()).await;
    }

    let dir = test_dir(&app, &test_id)?;
    let chunk_files = list_wavs(&dir).await?;
    if chunk_files.is_empty() {
        let _ = tokio::fs::remove_dir_all(test_root(&app, &test_id)?).await;
        return Err(Error::Other("녹음된 오디오가 없습니다".into()));
    }

    let out = test_root(&app, &test_id)?.join("test.webm");
    ffmpeg::concat_wavs_to_webm(&chunk_files, &out).await?;
    let bytes = tokio::fs::read(&out).await?;

    let _ = tokio::fs::remove_dir_all(test_root(&app, &test_id)?).await;
    Ok(bytes)
}

// ---- helpers ----

fn test_root(app: &AppHandle, test_id: &str) -> Result<PathBuf> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| Error::Other(format!("app_data_dir resolve failed: {e}")))?;
    Ok(base.join("test-captures").join(test_id))
}

fn test_dir(app: &AppHandle, test_id: &str) -> Result<PathBuf> {
    Ok(test_root(app, test_id)?.join("chunks"))
}

async fn list_wavs(dir: &PathBuf) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    let mut entries = tokio::fs::read_dir(dir).await?;
    while let Some(e) = entries.next_entry().await? {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) == Some("wav") {
            out.push(p);
        }
    }
    out.sort();
    Ok(out)
}
