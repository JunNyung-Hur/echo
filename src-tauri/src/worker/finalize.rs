//! Recording finalize task (P1-08).
//!
//! Replaces old `worker/tasks/finalize.py`:
//!   1. List all WAV chunks `<recording_dir>/chunks/<seq:06d>.wav`.
//!   2. ffmpeg concat demuxer + libopus re-encode → a single Opus-in-WebM
//!      (`ffmpeg::concat_wavs_to_webm`). WAV chunks carry their own header so
//!      they can't be byte-concatenated; the demuxer stitches at the container
//!      level, then re-encodes.
//!   3. Update DB: format='webm', file_path, original_filename, duration
//!      (probed via ffprobe — best-effort, None if ffprobe absent; G-REC-010).
//!   4. Post `recording_stopped` timeline event.
//!
//! Failure modes (all preserve chunks on disk — G-REC-011 — so user can
//! install ffmpeg / fix permissions and retry):
//!   - ffmpeg not on PATH → format='failed'.
//!   - No chunks present → format='failed'.
//!   - ffmpeg non-zero exit → format='failed'.

use std::path::PathBuf;

use tauri::AppHandle;
use tauri::Manager;

use crate::error::{Error, Result};
use crate::ffmpeg;
use crate::repo::recordings as recordings_repo;
use crate::timeline;
use crate::AppState;

/// Spawn the finalize task. Caller has already flipped the recording to
/// `format='finalizing'` via `commands::recordings::stop_recording`.
///
/// invariant: G-TASK-001 — DB transition (`recording`→`finalizing`) is
/// committed before this spawn; the worker reads its own row immediately.
pub fn spawn(app: AppHandle, recording_id: String) {
    tauri::async_runtime::spawn(async move {
        if let Err(err) = run(app.clone(), recording_id.clone()).await {
            tracing::error!(?err, %recording_id, "finalize task failed");
            // best-effort failure marking
            if let Some(state) = app.try_state::<AppState>() {
                let _ = recordings_repo::mark_failed(&state.db, &recording_id).await;
            }
        }
    });
}

async fn run(app: AppHandle, recording_id: String) -> Result<()> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| Error::Other("AppState missing".into()))?;
    let pool = state.db.clone();

    let rec = recordings_repo::get(&pool, &recording_id).await?;
    // G-TASK-002 — idempotent: already done.
    if rec.format == "webm" {
        return Ok(());
    }

    let chunk_dir = chunk_dir_for(&app, &rec.note_id, &recording_id)?;
    let chunks = list_sorted_chunks(&chunk_dir).await?;
    if chunks.is_empty() {
        recordings_repo::mark_failed(&pool, &recording_id).await?;
        return Err(Error::Other("no chunks present".into()));
    }

    let recording_dir = chunk_dir
        .parent()
        .ok_or_else(|| Error::Other("chunk_dir parent missing".into()))?;
    let output_filename = format!("{}.webm", recording_id);
    let output_path = recording_dir.join(&output_filename);

    // ffmpeg required to concat WAV chunks → Opus/WebM. Absent → fail but keep
    // chunks on disk (G-REC-011) so the user can install ffmpeg and retry.
    if !ffmpeg::is_available().await {
        recordings_repo::mark_failed(&pool, &recording_id).await?;
        return Err(Error::Other(
            "ffmpeg not found on PATH — install ffmpeg and retry".into(),
        ));
    }

    if let Err(err) = ffmpeg::concat_wavs_to_webm(&chunks, &output_path).await {
        recordings_repo::mark_failed(&pool, &recording_id).await?;
        return Err(err);
    }

    // Final webm is what matters — drop the WAV chunk dir.
    let _ = tokio::fs::remove_dir_all(&chunk_dir).await;

    // DB transition. Probe duration (best-effort — None if ffprobe absent).
    let file_path_str = output_path.to_string_lossy().to_string();
    let duration = ffmpeg::probe_duration(&output_path).await;
    recordings_repo::mark_finalized(
        &pool,
        &recording_id,
        &file_path_str,
        &output_filename,
        "webm",
        duration,
    )
    .await?;

    // 6) Timeline.
    timeline::post(
        &pool,
        &rec.note_id,
        "recording_stopped",
        "녹음이 정리되었습니다.",
    )
    .await?;

    tracing::info!(%recording_id, ?output_path, "finalize complete");

    // G-TASK-003 — auto-chain to transcribe (Phase 2). Best-effort: a chain
    // failure here must not undo a successful finalize.
    // freeform 노트는 자동 전사를 타지 않는다 — 녹음을 채팅에 첨부했다가 전송 시점에
    // 전사한다(Step 3/4). minutes만 stop→전사→회의록 자동 체인을 탄다.
    let skip_transcribe = matches!(
        crate::repo::notes::get(&pool, &rec.note_id).await,
        Ok(n) if n.note_type.as_deref() == Some("freeform")
    );
    if !skip_transcribe {
        if let Err(e) =
            crate::worker::transcribe::dispatch(&app, &pool, &rec.note_id, Some(&recording_id)).await
        {
            tracing::warn!(?e, %recording_id, "failed to auto-dispatch transcribe");
        }
    }

    Ok(())
}

async fn list_sorted_chunks(chunk_dir: &PathBuf) -> Result<Vec<PathBuf>> {
    let mut entries = tokio::fs::read_dir(chunk_dir).await?;
    let mut out: Vec<PathBuf> = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("wav") {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

fn chunk_dir_for(app: &AppHandle, note_id: &str, recording_id: &str) -> Result<PathBuf> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| Error::Other(format!("app_data_dir resolve failed: {e}")))?;
    Ok(base
        .join("recordings")
        .join(note_id)
        .join(recording_id)
        .join("chunks"))
}
