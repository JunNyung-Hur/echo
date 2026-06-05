//! notes Tauri commands.
//!
//! Thin layer over `crate::repo::notes` — the command handlers translate
//! frontend JSON ↔ repo input/output structs and own all transactional
//! side-effects that go beyond a single repo call (e.g. seeding the welcome
//! chat message on note create — F-NOTE-005).

use sqlx::SqlitePool;
use tauri::State;
use uuid::Uuid;

use crate::error::Result;
use crate::models::Note;
use crate::repo::notes::{CreateNoteInput, ListNotesQuery, ListNotesResponse, UpdateNoteInput};
use crate::repo::{
    note_bodies as note_bodies_repo, notes as notes_repo, transcripts as transcripts_repo,
};
use crate::AppState;

/// Second Brain 톤 — old corporate welcome line replaced (D-001). 생성 시점의
/// ui_lang(설정)에 맞춰 ko/en 중 하나를 시드한다(4a3b683).
const WELCOME_FREEFORM_KO: &str = "무엇이든 편하게 말씀하세요. 노트에 대신 받아적고 정리해드릴게요.";
const WELCOME_FREEFORM_EN: &str = "Just say what's on your mind — I'll write it down and tidy it up for you.";
const WELCOME_MINUTES_KO: &str = "녹음을 시작하거나 내용을 보내주시면 회의록으로 정리해드릴게요.";
const WELCOME_MINUTES_EN: &str = "Start a recording or send your notes — I'll turn them into minutes.";

#[tauri::command]
pub async fn create_note(state: State<'_, AppState>, input: CreateNoteInput) -> Result<Note> {
    let note = notes_repo::create(&state.db, input).await?;
    seed_welcome_message(&state.db, &note.id, note.note_type.as_deref()).await?;
    Ok(note)
}

#[tauri::command]
pub async fn list_notes(
    state: State<'_, AppState>,
    query: ListNotesQuery,
) -> Result<ListNotesResponse> {
    notes_repo::list(&state.db, query).await
}

#[tauri::command]
pub async fn get_note(state: State<'_, AppState>, id: String) -> Result<Note> {
    notes_repo::get(&state.db, &id).await
}

#[tauri::command]
pub async fn update_note(
    state: State<'_, AppState>,
    id: String,
    input: UpdateNoteInput,
) -> Result<Note> {
    notes_repo::update(&state.db, &id, input).await
}

/// FK CASCADE handles dependent *rows* (recordings/transcripts/note_bodies/
/// chat/timeline/tags) per G-DB-002 — but NOT files on disk. Gather the artifact
/// ids first, delete the rows, then best-effort remove the on-disk dirs so a
/// deleted note doesn't leak webm / transcript / body files.
#[tauri::command]
pub async fn delete_note(state: State<'_, AppState>, id: String) -> Result<()> {
    let pool = state.db.clone();

    let transcripts = transcripts_repo::list_for_note(&pool, &id)
        .await
        .unwrap_or_default();
    let bodies = note_bodies_repo::list_for_note(&pool, &id)
        .await
        .unwrap_or_default();

    // G-CANCEL-007 — cancel in-flight transcribe/generate tasks before deleting,
    // so a running task doesn't keep working (and writing) against rows that are
    // about to vanish. The worker polls its flag at each checkpoint (G-CANCEL-002)
    // and exits as Cancelled. (invariant: G-CANCEL-007)
    for t in &transcripts {
        if let Some(tid) = &t.task_id {
            state.cancellations.cancel(tid);
        }
    }
    for b in &bodies {
        if let Some(tid) = &b.task_id {
            state.cancellations.cancel(tid);
        }
    }

    notes_repo::delete(&pool, &id).await?;

    // Note-centric layout — a note's recordings/transcripts/bodies all live under
    // one id-derived folder, so removing it cleans every artifact in one shot.
    let _ = tokio::fs::remove_dir_all(crate::storage::note_abs_dir(&id)).await;
    Ok(())
}

/// Absolute path of a note's on-disk folder (`notes/<dir_name>/`) — the UI's
/// "open folder" button opens this. Creates it if missing so the open never
/// fails on a note that has no artifacts yet.
#[tauri::command]
pub async fn note_folder_path(state: State<'_, AppState>, id: String) -> Result<String> {
    let _ = notes_repo::get(&state.db, &id).await?; // 404 on an unknown id
    let dir = crate::storage::note_abs_dir(&id);
    let _ = std::fs::create_dir_all(&dir);
    Ok(dir.to_string_lossy().to_string())
}

/// F-NOTE-005 — drop one assistant row into the chat so the panel is never
/// empty on first open. Phase 3 will repurpose this surface for the actual
/// agent; for now it's a static greeting carried over from old
/// `meeting_service.INITIAL_ASSISTANT_MESSAGE` but with Second Brain wording.
async fn seed_welcome_message(pool: &SqlitePool, note_id: &str, note_type: Option<&str>) -> Result<()> {
    // 생성 시점 ui_lang(설정) + note_type에 맞춘 welcome. 이후 대화는 사용자 발화
    // 언어를 LLM이 자연히 따라간다.
    let ui_lang = crate::repo::settings::get(pool, "ui_lang").await.ok().flatten();
    let en = ui_lang.as_deref() == Some("en");
    let msg = match (note_type, en) {
        (Some("minutes"), false) => WELCOME_MINUTES_KO,
        (Some("minutes"), true) => WELCOME_MINUTES_EN,
        (_, true) => WELCOME_FREEFORM_EN,
        (_, false) => WELCOME_FREEFORM_KO,
    };
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO note_chat_messages (id, note_id, role, content) VALUES (?, ?, 'assistant', ?)",
    )
    .bind(&id)
    .bind(note_id)
    .bind(msg)
    .execute(pool)
    .await?;
    Ok(())
}
