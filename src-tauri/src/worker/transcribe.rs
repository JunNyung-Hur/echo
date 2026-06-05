//! Transcribe worker (Phase 2) — ports worker/app/tasks/transcribe.py.
//!
//! Chain position: finalize → **transcribe** → generate (G-TASK-003).
//!
//! Guards:
//!   - G-TASK-001  transcript row committed (status='processing', task_id) by
//!     `dispatch` before the worker spawns.
//!   - G-TASK-008  per-chunk ASR retry — 3 attempts, exponential backoff.
//!   - G-CANCEL-002  cancellation poll before each chunk.
//!   - G-CANCEL-005  on cancel → status='cancelled' + timeline event.
//!   - G-TASK-010  whole task wrapped in TASK_TIME_LIMIT.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::asr;
use crate::error::{Error, Result};
use crate::repo::{ai_endpoints, notes, recordings, transcripts};
use crate::timeline;
use crate::worker::{check_cancelled, TASK_TIME_LIMIT};
use crate::AppState;

#[derive(Clone, Serialize)]
struct ProgressPayload {
    note_id: String,
    transcript_id: String,
    current: usize,
    total: usize,
    /// "finalize" | "transcribe" | "minutes"
    stage: String,
}

fn emit_progress(
    app: &AppHandle,
    note_id: &str,
    transcript_id: &str,
    current: usize,
    total: usize,
    stage: &str,
) {
    let _ = app.emit(
        "transcribe:progress",
        ProgressPayload {
            note_id: note_id.to_string(),
            transcript_id: transcript_id.to_string(),
            current,
            total,
            stage: stage.to_string(),
        },
    );
}

/// G-TASK-001 — create a `processing` transcript row (pre-allocated task_id) in
/// a single commit, then spawn the worker. Called by finalize (auto-chain).
pub async fn dispatch(
    app: &AppHandle,
    pool: &SqlitePool,
    note_id: &str,
    recording_id: Option<&str>,
) -> Result<()> {
    let task_id = Uuid::new_v4().to_string();
    let t = transcripts::create_processing(pool, note_id, recording_id, &task_id).await?;
    spawn(app.clone(), t.id, task_id);
    Ok(())
}

pub fn spawn(app: AppHandle, transcript_id: String, task_id: String) {
    // Extract owned handles synchronously — never hold tauri::State across await.
    let (pool, flag) = match app.try_state::<AppState>() {
        Some(s) => (s.db.clone(), s.cancellations.register(task_id.clone())),
        None => return,
    };

    tauri::async_runtime::spawn(async move {
        let outcome = tokio::time::timeout(
            TASK_TIME_LIMIT,
            run(app.clone(), pool.clone(), transcript_id.clone(), flag),
        )
        .await;

        match outcome {
            Ok(Ok(())) => {}
            Ok(Err(Error::Cancelled)) => {
                finish(
                    &pool,
                    &transcript_id,
                    "cancelled",
                    "transcribe_cancelled",
                    "전사가 취소되었습니다.",
                )
                .await;
            }
            Ok(Err(e)) => {
                tracing::error!(?e, %transcript_id, "transcribe failed");
                finish(
                    &pool,
                    &transcript_id,
                    "failed",
                    "transcribe_failed",
                    "전사 중 오류가 발생했습니다.",
                )
                .await;
            }
            Err(_) => {
                tracing::error!(%transcript_id, "transcribe timed out");
                finish(
                    &pool,
                    &transcript_id,
                    "failed",
                    "transcribe_failed",
                    "전사 시간 초과로 중단되었습니다.",
                )
                .await;
            }
        }

        // G-CANCEL — drop the flag once the task is done.
        if let Some(s) = app.try_state::<AppState>() {
            s.cancellations.unregister(&task_id);
        }
        let _ = app.emit("note:updated", transcript_id.clone());
    });
}

/// Mark the transcript's terminal status + post a timeline event (best-effort).
async fn finish(pool: &SqlitePool, transcript_id: &str, status: &str, kind: &str, msg: &str) {
    // Idempotent-ish: don't clobber a row that already reached a terminal state.
    if let Ok(t) = transcripts::get(pool, transcript_id).await {
        if t.status == "completed" {
            return;
        }
        let _ = transcripts::mark_status(pool, transcript_id, status).await;
        let _ = timeline::post(pool, &t.note_id, kind, msg).await;
    }
}

async fn run(
    app: AppHandle,
    pool: SqlitePool,
    transcript_id: String,
    flag: Arc<AtomicBool>,
) -> Result<()> {
    let t = transcripts::get(&pool, &transcript_id).await?;
    let note = notes::get(&pool, &t.note_id).await?;
    let language = note.language.clone();

    tracing::info!(%transcript_id, note_id = %t.note_id, "transcribe: started");
    let _ = timeline::post(
        &pool,
        &t.note_id,
        "transcribe_started",
        "전사가 시작되었습니다.",
    )
    .await;
    let _ = app.emit("note:updated", t.note_id.clone());
    emit_progress(&app, &t.note_id, &transcript_id, 0, 1, "finalize");

    let recording_id = t
        .recording_id
        .clone()
        .ok_or_else(|| Error::Other("transcript has no recording".into()))?;
    let rec = recordings::get(&pool, &recording_id).await?;

    let asr_ep = ai_endpoints::get_active(&pool, "asr")
        .await?
        .ok_or_else(|| Error::Other("활성 ASR endpoint가 없습니다. 설정에서 등록하세요.".into()))?;

    let chunk_seconds = asr_ep.chunk_seconds.unwrap_or(300).max(1) as u32;
    let max_tokens = asr_ep.max_tokens.unwrap_or(4096);

    // ffmpeg: webm → 16kHz mono WAV chunks. Flat note-centric storage; the DB
    // stores app_data-relative paths.
    let chunk_dir =
        crate::storage::resolve(&crate::storage::transcript_chunks_rel(&t.note_id, &transcript_id));
    let rec_path = crate::storage::resolve(&rec.file_path);
    let chunks = asr::split_to_wav_chunks(&rec_path, &chunk_dir, chunk_seconds).await?;
    if chunks.is_empty() {
        return Err(Error::Other("no audio chunks produced".into()));
    }
    let total = chunks.len();

    let mut texts: Vec<String> = Vec::new();
    for (i, chunk_path) in chunks.iter().enumerate() {
        check_cancelled(&flag)?; // G-CANCEL-002 — before each chunk
        emit_progress(&app, &t.note_id, &transcript_id, i, total, "transcribe");

        let wav = tokio::fs::read(chunk_path).await?;
        let duration = asr::wav_duration_secs(wav.len());

        // G-TASK-008 — 3 attempts, exponential backoff.
        let mut raw: Option<String> = None;
        for attempt in 0..3 {
            match asr::asr_chunk(
                &asr_ep,
                &asr_ep.request_mode,
                &wav,
                duration,
                &language,
                max_tokens,
            )
            .await
            {
                Ok(text) => {
                    raw = text;
                    break;
                }
                Err(e) => {
                    if attempt == 2 {
                        tracing::warn!(chunk = i, error = %e, "ASR chunk failed after 3 attempts");
                    } else {
                        tokio::time::sleep(Duration::from_secs_f64(2.0 * 2.0_f64.powi(attempt)))
                            .await;
                    }
                }
            }
        }

        if let Some(raw_text) = raw {
            // eb0b667 — store ASR raw output directly. The LLM post-process
            // (normalizer) was removed: it degraded quality vs raw ASR and had
            // long been disabled in production.
            if !raw_text.trim().is_empty() {
                texts.push(raw_text);
            }
        }
        // Chunk done — advance to *completed*-chunk count (Meetzy uses
        // current/total of completed chunks). The per-chunk-start emit above
        // anchors the transcribe step; this one lets it actually reach 95% on
        // the last chunk instead of stalling at the start value (~5% with one
        // chunk).
        emit_progress(&app, &t.note_id, &transcript_id, i + 1, total, "transcribe");
    }

    let full = texts.join("\n\n");
    if full.trim().is_empty() {
        return Err(Error::Other(
            "ASR produced no output — all chunks empty or failed".into(),
        ));
    }

    // Persist transcript text + complete; store the app_data-relative path, then
    // drop the transient wav chunks.
    let text_rel = crate::storage::transcript_text_rel(&t.note_id, &transcript_id);
    let text_abs = crate::storage::resolve(&text_rel);
    if let Some(parent) = text_abs.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&text_abs, full.as_bytes()).await?;
    transcripts::set_paths_and_complete(&pool, &transcript_id, &text_rel, Some(&text_rel)).await?;
    let _ = tokio::fs::remove_dir_all(&chunk_dir).await;
    tracing::info!(%transcript_id, chunks = total, chars = full.len(), "transcribe: completed");
    let _ = timeline::post(
        &pool,
        &t.note_id,
        "transcribe_completed",
        "전사가 완료되었습니다.",
    )
    .await;
    let _ = app.emit("note:updated", t.note_id.clone());
    // F-DESKTOP-004 — ping the user (app may be tray-minimized).
    crate::worker::notify(&app, &pool, "notify_transcribe", "전사 완료", &format!("{} — 본문을 정리하고 있어요", note.title)).await;

    // Tidy the chunk WAVs (keep raw.txt).
    let _ = tokio::fs::remove_dir_all(&chunk_dir).await;

    check_cancelled(&flag)?; // G-CANCEL — before chaining

    // G-TASK-003 — auto-chain to generate (minutes). freeform 노트는 채팅에 첨부한
    // 녹음을 전사하는 것이라 회의록 생성을 타지 않는다 — 전송 핸들러(run_attachment_turn)
    // 가 전사 텍스트를 run_write로 노트에 종합한다.
    if note.note_type.as_deref() != Some("freeform") {
        if let Err(e) =
            crate::worker::generate::dispatch(&app, &pool, &t.note_id, &transcript_id, &text_rel)
                .await
        {
            tracing::warn!(?e, note_id = %t.note_id, "failed to auto-dispatch generate");
        }
    }

    Ok(())
}
