//! Phase 2 processing commands — transcript/body status for stage derivation,
//! note-body content read, and transcribe retry (F-TRANS-005 / F-STAGE-003).

use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::models::{NoteBody, TimelineEvent, Transcript};
use crate::repo::{ai_endpoints, note_bodies, recordings, transcripts};
use crate::AppState;

#[tauri::command]
pub async fn list_transcripts(
    state: State<'_, AppState>,
    note_id: String,
) -> Result<Vec<Transcript>> {
    transcripts::list_for_note(&state.db, &note_id).await
}

/// F-VIEW (1f207ab) — full transcript text for the TranscriptViewerModal. The
/// chat read_transcript tool only inlines a 1K preview; the 전체보기 modal
/// fetches the complete text here on demand by transcript_id.
#[tauri::command]
pub async fn get_transcript_content(
    state: State<'_, AppState>,
    transcript_id: String,
) -> Result<String> {
    let t = transcripts::get(&state.db, &transcript_id).await?;
    let path = t
        .corrected_path
        .or(t.raw_path)
        .ok_or_else(|| Error::Other("전사록 파일 경로가 없습니다.".into()))?;
    Ok(tokio::fs::read_to_string(crate::storage::resolve(&path)).await?)
}

#[tauri::command]
pub async fn list_note_bodies(
    state: State<'_, AppState>,
    note_id: String,
) -> Result<Vec<NoteBody>> {
    note_bodies::list_for_note(&state.db, &note_id).await
}

/// Lifecycle events (transcribe/minutes started·completed·failed) for the
/// chat timeline pills (G-LIFE-001).
#[tauri::command]
pub async fn list_timeline(
    state: State<'_, AppState>,
    note_id: String,
) -> Result<Vec<TimelineEvent>> {
    Ok(sqlx::query_as::<_, TimelineEvent>(
        "SELECT * FROM note_timeline_events WHERE note_id = ? ORDER BY created_at ASC, rowid ASC",
    )
    .bind(&note_id)
    .fetch_all(&state.db)
    .await?)
}

/// Read a note body's rendered HTML from disk (content_path). `None` if the
/// body has no content yet or the file is unreadable.
#[tauri::command]
pub async fn get_body_content(
    state: State<'_, AppState>,
    body_id: String,
) -> Result<Option<String>> {
    let body = note_bodies::get(&state.db, &body_id).await?;
    match body.content_path {
        Some(p) if !p.is_empty() => {
            Ok(tokio::fs::read_to_string(crate::storage::resolve(&p)).await.ok())
        }
        _ => Ok(None),
    }
}

/// Restore an archived body version into a new active row (F-VERSION-001 /
/// G-VERSION-001). Copies the selected version's content into a fresh file,
/// archives the current active, and makes the copy the new active — the source
/// version stays archived so it can be re-restored. Meeting context is NOT
/// rolled back; only the body content.
#[tauri::command]
pub async fn restore_note_body(
    state: State<'_, AppState>,
    note_id: String,
    body_id: String,
) -> Result<()> {
    let pool = state.db.clone();
    let target = note_bodies::get(&pool, &body_id).await?;
    let target_path = target
        .content_path
        .clone()
        .ok_or_else(|| Error::Other("선택한 버전에 본문이 없습니다.".into()))?;
    let html = tokio::fs::read_to_string(crate::storage::resolve(&target_path)).await?;

    // New row's context_snapshot + initial baseline come from the *current*
    // active (context isn't rolled back); fall back to the target if no active.
    let active = note_bodies::get_active(&pool, &note_id).await?;
    let src = active.as_ref().unwrap_or(&target);
    let context_snapshot = src.context_snapshot.clone();
    let initial_content = src
        .initial_content_path
        .clone()
        .or_else(|| src.content_path.clone());
    let initial_ctx = src
        .initial_context_snapshot
        .clone()
        .or_else(|| Some(src.context_snapshot.clone()));
    let transcript_id = src.transcript_id.clone();

    let new_id = Uuid::new_v4().to_string();
    let content_rel = crate::storage::body_rel(&note_id, &new_id);
    let path = crate::storage::resolve(&content_rel);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, html.as_bytes()).await?;

    note_bodies::archive_and_create_completed(
        &pool,
        &new_id,
        &note_id,
        transcript_id.as_deref(),
        &content_rel,
        &context_snapshot,
        initial_content.as_deref(),
        initial_ctx.as_deref(),
        false,
    )
    .await?;
    Ok(())
}

/// Save a manual edit of the active body as a new version (F-VIEW / G-CHAT-004).
/// Archives the current active + creates a new completed body flagged
/// is_manual_edit (so the version history shows a "직접 수정" badge).
#[tauri::command]
pub async fn save_manual_body_edit(
    state: State<'_, AppState>,
    note_id: String,
    html: String,
) -> Result<()> {
    if html.trim().is_empty() {
        return Err(Error::Other("본문이 비어 있습니다.".into()));
    }
    let pool = state.db.clone();
    // 빈 노트(활성 본문 없음)에서도 '수정'으로 첫 본문을 만들 수 있다 — run_write와 동일하게
    // note 메타로 context_snapshot을 만들고, active가 없으면 initial/transcript는 None.
    let note = crate::repo::notes::get(&pool, &note_id).await?;
    let active = note_bodies::get_active(&pool, &note_id).await?;
    let context_snapshot = crate::worker::generate::context_snapshot_json(&note);
    let (initial_content, initial_ctx) = match &active {
        Some(a) => (
            a.initial_content_path.clone().or_else(|| a.content_path.clone()),
            a.initial_context_snapshot
                .clone()
                .or_else(|| Some(a.context_snapshot.clone())),
        ),
        None => (None, None),
    };

    let new_id = Uuid::new_v4().to_string();
    let content_rel = crate::storage::body_rel(&note_id, &new_id);
    let path = crate::storage::resolve(&content_rel);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, html.as_bytes()).await?;

    note_bodies::archive_and_create_completed(
        &pool,
        &new_id,
        &note_id,
        active.as_ref().and_then(|a| a.transcript_id.as_deref()),
        &content_rel,
        &context_snapshot,
        initial_content.as_deref(),
        initial_ctx.as_deref(),
        true,
    )
    .await?;
    Ok(())
}

/// Retry transcription for a note (F-TRANS-005). Cleans up prior transcripts +
/// bodies, then re-dispatches the chain from the note's finalized recording.
#[tauri::command]
pub async fn retry_transcribe(
    app: AppHandle,
    state: State<'_, AppState>,
    note_id: String,
) -> Result<()> {
    let pool = state.db.clone();

    // Precondition (surfaced clearly instead of failing silently mid-chain):
    // an endpoint must be both registered AND activated (⭐) — create leaves
    // is_active=0, so registration alone isn't enough.
    if ai_endpoints::get_active(&pool, "asr").await?.is_none() {
        return Err(Error::Other(
            "활성 ASR endpoint가 없습니다. 설정(⚙)에서 ASR endpoint를 등록하고 ⭐로 활성화해주세요.".into(),
        ));
    }
    if ai_endpoints::get_active(&pool, "llm").await?.is_none() {
        return Err(Error::Other(
            "활성 LLM endpoint가 없습니다. 설정(⚙)에서 LLM endpoint를 등록하고 ⭐로 활성화해주세요.".into(),
        ));
    }

    let recs = recordings::list_for_note(&pool, &note_id).await?;
    let rec = recs
        .into_iter()
        .find(|r| r.format == "webm")
        .ok_or_else(|| Error::Other("정리된 녹음이 없어 전사를 재시도할 수 없습니다.".into()))?;

    // Fresh start — drop prior artifacts so stage derivation isn't confused by
    // a stale failed transcript/body.
    for b in note_bodies::list_for_note(&pool, &note_id).await? {
        let _ = note_bodies::delete(&pool, &b.id).await;
    }
    for t in transcripts::list_for_note(&pool, &note_id).await? {
        let _ = transcripts::delete(&pool, &t.id).await;
    }

    crate::worker::transcribe::dispatch(&app, &pool, &note_id, Some(&rec.id)).await
}
