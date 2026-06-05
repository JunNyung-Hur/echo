//! recordings Tauri commands (native cpal capture — D-023 / P1R-02).
//!
//! - start_recording: insert row + create chunk dir + start cpal capture session.
//!   The capture thread writes 5s WAV chunks directly to disk (G-REC-001) and
//!   touches last_chunk_at on each flush (G-REC-002).
//! - stop_recording: stop the capture (flushes final chunk) → mark `finalizing`
//!   → spawn finalize task.
//! - delete_recording: stop any live capture, remove DB row + chunk dir.
//!
//! Chunk file layout (G-REC-001):
//!   <app_data_dir>/notes/<dir_name>/recordings/<recording_id>/chunks/<seq:06d>.wav

use std::path::PathBuf;

use tauri::{AppHandle, Emitter, State};
use tokio::fs;

use crate::audio_capture;
use crate::error::{Error, Result};
use crate::models::Recording;
use crate::repo::recordings as recordings_repo;
use crate::repo::recordings::StartRecordingInput;
use crate::AppState;

// ============================================================================
// Reads
// ============================================================================

#[tauri::command]
pub async fn list_recordings(
    state: State<'_, AppState>,
    note_id: String,
) -> Result<Vec<Recording>> {
    recordings_repo::list_for_note(&state.db, &note_id).await
}

/// Finalized recordings attached to this note but not yet sent — restored as
/// chips when the user re-enters the freeform note (Step 3/4 recovery).
#[tauri::command]
pub async fn list_pending_recordings(
    state: State<'_, AppState>,
    note_id: String,
) -> Result<Vec<Recording>> {
    recordings_repo::list_pending_for_note(&state.db, &note_id).await
}

/// Recordings already sent on this note — the 보관함(archive) history.
#[tauri::command]
pub async fn list_archived_recordings(
    state: State<'_, AppState>,
    note_id: String,
) -> Result<Vec<Recording>> {
    recordings_repo::list_archived_for_note(&state.db, &note_id).await
}

#[tauri::command]
pub async fn get_recording(state: State<'_, AppState>, id: String) -> Result<Recording> {
    recordings_repo::get(&state.db, &id).await
}

/// Read a finalized recording's audio bytes for in-app playback. Returned as a
/// raw IPC `Response` (an `ArrayBuffer` on the JS side) so the UI can wrap it in
/// a Blob URL — no asset-protocol scope config needed for app-data files.
#[tauri::command]
pub async fn read_recording_audio(
    state: State<'_, AppState>,
    id: String,
) -> Result<tauri::ipc::Response> {
    let rec = recordings_repo::get(&state.db, &id).await?;
    if rec.file_path.is_empty() {
        return Err(Error::Other("녹음이 아직 준비되지 않았어요.".into()));
    }
    let bytes = fs::read(crate::storage::resolve(&rec.file_path))
        .await
        .map_err(|e| Error::Other(format!("녹음 파일을 읽을 수 없어요: {e}")))?;
    Ok(tauri::ipc::Response::new(bytes))
}

// ============================================================================
// Lifecycle (native capture)
// ============================================================================

/// Start a native capture session.
///
/// `device_name` + `source` ("mic" | "system") identify the unified input
/// source the user picked. G-REC-007 (one recording per note) is enforced in
/// the repo. The cpal capture thread owns chunk writing; we just hold its
/// handle in AppState so stop_recording can end it.
#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    note_id: String,
    device_name: String,
    source: String,
) -> Result<Recording> {
    let recording = recordings_repo::start(
        &state.db,
        StartRecordingInput {
            note_id: note_id.clone(),
        },
    )
    .await?;

    let chunk_dir = chunk_dir_for(&note_id, &recording.id);
    fs::create_dir_all(&chunk_dir).await?;

    // on_flush runs on the capture thread every 5s — touch last_chunk_at
    // (G-REC-002 heartbeat) via a short block_on against the shared pool.
    let db = state.db.clone();
    let rid = recording.id.clone();
    let on_flush = move |_seq: u32| {
        let db = db.clone();
        let rid = rid.clone();
        tauri::async_runtime::block_on(async move {
            let _ = recordings_repo::touch_last_chunk(&db, &rid).await;
        });
    };

    let handle = audio_capture::start_capture(
        app.clone(),
        device_name,
        source,
        chunk_dir,
        recording.id.clone(),
        on_flush,
    )
    .map_err(|e| {
        // Capture failed to open — roll the row back to a failed state so the
        // UI doesn't get stuck in `recording` with no live session.
        tracing::warn!(recording_id = %recording.id, error = %e, "capture start failed");
        Error::Other(e)
    })?;

    if let Ok(mut map) = state.captures.lock() {
        map.insert(recording.id.clone(), handle);
    }

    tracing::info!(recording_id = %recording.id, "recording + capture started");
    Ok(recording)
}

/// Stop the capture (flushes the final chunk), then mark `finalizing` and spawn
/// the finalize task (G-TASK-001: DB transition committed before spawn).
#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    recording_id: String,
) -> Result<()> {
    // Pull the handle out under the lock, then stop() (joins the thread) off
    // the async runtime so we don't block the executor.
    let handle = {
        match state.captures.lock() {
            Ok(mut map) => map.remove(&recording_id),
            Err(_) => None,
        }
    };
    if let Some(h) = handle {
        let _ = tauri::async_runtime::spawn_blocking(move || h.stop()).await;
    }

    recordings_repo::mark_finalizing(&state.db, &recording_id).await?;
    tracing::info!(recording_id = %recording_id, "recording stopped — spawning finalize");
    crate::worker::finalize::spawn(app, recording_id);
    Ok(())
}

/// F-REC-004 — import an external audio file as this note's recording.
///
/// Converts the picked file to Opus/WebM (same shape cpal-finalize produces) so
/// it rides the *identical* transcribe→generate chain — no format special-cases
/// downstream. G-REC-007 (one recording per note) is enforced via repo::start.
/// On conversion failure the row is dropped (unlike cpal there are no chunks to
/// preserve, so leaving a `failed` row would just clutter the note).
#[tauri::command]
pub async fn import_audio_file(
    app: AppHandle,
    state: State<'_, AppState>,
    note_id: String,
    src_path: String,
) -> Result<Recording> {
    let pool = state.db.clone();

    // ffmpeg required — fail clearly *before* creating a row if it's absent.
    if !crate::ffmpeg::is_available().await {
        return Err(Error::Other(
            "ffmpeg가 설치되어 있지 않아 파일을 가져올 수 없어요. ffmpeg 설치 후 다시 시도해주세요.".into(),
        ));
    }
    let src = PathBuf::from(&src_path);
    if !src.exists() {
        return Err(Error::Other("선택한 파일을 찾을 수 없어요.".into()));
    }
    let original_filename = src
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "imported-audio".into());

    // G-REC-007 — one recording per note (repo enforces; errors if one exists).
    let recording = recordings_repo::start(
        &pool,
        StartRecordingInput {
            note_id: note_id.clone(),
        },
    )
    .await?;

    let rel_path = crate::storage::recording_webm_rel(&note_id, &recording.id);
    let output = crate::storage::resolve(&rel_path);
    if let Err(e) = crate::ffmpeg::convert_to_webm(&src, &output).await {
        let _ = recordings_repo::delete(&pool, &recording.id).await;
        return Err(e);
    }

    let duration = crate::ffmpeg::probe_duration(&output).await;
    recordings_repo::mark_finalized(
        &pool,
        &recording.id,
        &rel_path,
        &original_filename,
        "webm",
        duration,
    )
    .await?;
    let _ = crate::timeline::post(
        &pool,
        &note_id,
        "recording_stopped",
        "오디오 파일을 가져왔습니다.",
    )
    .await;
    let _ = app.emit("note:updated", note_id.clone());
    tracing::info!(recording_id = %recording.id, %note_id, "audio file imported");

    // freeform 노트는 자동 전사를 건너뛴다 — 채팅에 첨부했다가 전송 시점에 전사한다
    // (finalize와 동일 규칙). minutes만 import→전사→회의록 자동 체인을 탄다.
    let skip_transcribe = matches!(
        crate::repo::notes::get(&pool, &note_id).await,
        Ok(n) if n.note_type.as_deref() == Some("freeform")
    );
    if !skip_transcribe {
        // Best-effort — a dispatch failure must not undo a successful import.
        if let Err(e) =
            crate::worker::transcribe::dispatch(&app, &pool, &note_id, Some(&recording.id)).await
        {
            tracing::warn!(?e, %note_id, "failed to dispatch transcribe after import");
        }
    }

    // `recording` was built by start() before finalize — its duration/format/
    // file_path are stale. Re-read the finalized row so the caller (chip) gets
    // the real duration.
    recordings_repo::get(&pool, &recording.id).await
}

/// Delete a recording: stop any live capture, then remove DB row + chunk dir.
#[tauri::command]
pub async fn delete_recording(
    state: State<'_, AppState>,
    recording_id: String,
) -> Result<()> {
    let handle = {
        match state.captures.lock() {
            Ok(mut map) => map.remove(&recording_id),
            Err(_) => None,
        }
    };
    if let Some(h) = handle {
        let _ = tauri::async_runtime::spawn_blocking(move || h.stop()).await;
    }

    let rec = recordings_repo::get(&state.db, &recording_id).await?;
    // Remove the finalized webm + any leftover capture chunks for this recording.
    let webm = crate::storage::resolve(&crate::storage::recording_webm_rel(&rec.note_id, &recording_id));
    let _ = fs::remove_file(&webm).await;
    let chunks = chunk_dir_for(&rec.note_id, &recording_id);
    if chunks.exists() {
        fs::remove_dir_all(&chunks).await.ok();
    }
    recordings_repo::delete(&state.db, &recording_id).await?;
    Ok(())
}

// ============================================================================
// helpers
// ============================================================================

/// Live-capture chunk dir (`notes/note-<id8>/recordings/<rec_id>.chunks/`).
fn chunk_dir_for(note_id: &str, recording_id: &str) -> PathBuf {
    crate::storage::resolve(&crate::storage::recording_chunks_rel(note_id, recording_id))
}
