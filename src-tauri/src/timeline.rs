//! Timeline event post helper (G-LIFE-001/002/004).
//!
//! Events live in `note_timeline_events` (separate from chat) and the kind
//! catalog is fixed by the CHECK constraint in 001_initial.sql. Caller
//! should post events atomically with the state change that triggered them.

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::Result;

// f1d32c2 — 타임라인 이벤트 영어 문구. 소유자 ui_lang이 en이면 한국어 content를
// 이걸로 치환(pill + LLM view 가 UI 언어와 일치). 미등록 kind 는 원본 유지.
fn timeline_en(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "transcribe_started" => "Transcription started.",
        "transcribe_completed" => "Transcription complete.",
        "transcribe_cancelled" => "Transcription cancelled.",
        "transcribe_failed" => "An error occurred during transcription.",
        "minutes_started" => "Generating the meeting minutes…",
        "minutes_generated" => "Meeting minutes are ready.",
        "minutes_cancelled" => "Minutes generation cancelled.",
        "minutes_failed" => "An error occurred while generating the minutes.",
        _ => return None,
    })
}

pub async fn post(pool: &SqlitePool, note_id: &str, kind: &str, content: &str) -> Result<()> {
    // ui_lang이 en이면 등록된 kind 문구를 영어로 치환(없으면 한국어 원본 유지).
    let localized: &str = if let Some(en) = timeline_en(kind) {
        let ui_lang = crate::repo::settings::get(pool, "ui_lang").await.ok().flatten();
        if ui_lang.as_deref() == Some("en") {
            en
        } else {
            content
        }
    } else {
        content
    };
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO note_timeline_events (id, note_id, kind, content) VALUES (?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(note_id)
    .bind(kind)
    .bind(localized)
    .execute(pool)
    .await?;
    Ok(())
}
