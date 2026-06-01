//! recordings repository.
//!
//! Recording state lifecycle (G-REC-001/002/007/011):
//!   `recording` → `finalizing` → `webm`/`mp3`/`wav`/`m4a`  (happy)
//!                                     ↓
//!                                  `failed` (chunks preserved on disk)
//!
//! File chunks themselves live under
//!   `<app_data_dir>/recordings/<note_id>/<recording_id>/chunks/<seq:06d>.webm`
//! and are written by `crate::commands::recordings` (not this module).

use serde::Deserialize;
use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::models::Recording;

#[derive(Debug, Deserialize)]
pub struct StartRecordingInput {
    pub note_id: String,
}

/// invariant: G-REC-007 — only one *in-progress* recording per note at a time.
/// Finalized recordings may accumulate (multi-recording attach in freeform chat);
/// the guard blocks only a second concurrent capture while one is still active.
pub async fn start(pool: &SqlitePool, input: StartRecordingInput) -> Result<Recording> {
    if has_active_recording(pool, &input.note_id).await? {
        return Err(Error::InvalidInput(
            "이미 진행 중인 녹음이 있어요. 먼저 멈춘 뒤 다시 시도해주세요.".into(),
        ));
    }
    let id = Uuid::new_v4().to_string();
    // file_path filled at finalize; placeholder empty string until then.
    sqlx::query(
        "INSERT INTO recordings (id, note_id, file_path, original_filename, format) \
         VALUES (?, ?, '', '', 'recording')",
    )
    .bind(&id)
    .bind(&input.note_id)
    .execute(pool)
    .await?;
    get(pool, &id).await
}

/// True while a capture is mid-flight (`recording`/`finalizing`). Finalized
/// recordings (format = webm/mp3/wav/m4a/failed) do not count — multiple may
/// coexist on one note.
pub async fn has_active_recording(pool: &SqlitePool, note_id: &str) -> Result<bool> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM recordings \
         WHERE note_id = ? AND format IN ('recording', 'finalizing')",
    )
    .bind(note_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0 > 0)
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Recording> {
    sqlx::query_as::<_, Recording>("SELECT * FROM recordings WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("recording {id}")))
}

pub async fn list_for_note(pool: &SqlitePool, note_id: &str) -> Result<Vec<Recording>> {
    Ok(sqlx::query_as::<_, Recording>(
        "SELECT * FROM recordings WHERE note_id = ? ORDER BY created_at ASC",
    )
    .bind(note_id)
    .fetch_all(pool)
    .await?)
}

/// Finalized recordings on this note not yet consumed by a chat send — restored
/// as attachment chips when the user re-enters the note (Step 3/4 recovery).
pub async fn list_pending_for_note(pool: &SqlitePool, note_id: &str) -> Result<Vec<Recording>> {
    Ok(sqlx::query_as::<_, Recording>(
        "SELECT * FROM recordings \
         WHERE note_id = ? AND consumed_at IS NULL AND finalized_at IS NOT NULL \
         ORDER BY created_at ASC",
    )
    .bind(note_id)
    .fetch_all(pool)
    .await?)
}

/// Recordings already sent/consumed on this note — the note's audio history
/// shown in the 보관함(archive). Finalized + consumed, oldest first.
pub async fn list_archived_for_note(pool: &SqlitePool, note_id: &str) -> Result<Vec<Recording>> {
    Ok(sqlx::query_as::<_, Recording>(
        "SELECT * FROM recordings \
         WHERE note_id = ? AND consumed_at IS NOT NULL AND finalized_at IS NOT NULL \
         ORDER BY created_at ASC",
    )
    .bind(note_id)
    .fetch_all(pool)
    .await?)
}

/// Link recordings to the chat message that sent them + stamp consumed (Step 4).
/// `chat_message_id` drives the attachment chips in the user's bubble; setting
/// `consumed_at` moves them out of the pending pool into the 보관함. No-op empty.
pub async fn link_to_message(pool: &SqlitePool, ids: &[String], message_id: &str) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let mut qb: QueryBuilder<Sqlite> =
        QueryBuilder::new("UPDATE recordings SET consumed_at = datetime('now'), chat_message_id = ");
    qb.push_bind(message_id);
    qb.push(" WHERE id IN (");
    let mut sep = qb.separated(", ");
    for id in ids {
        sep.push_bind(id);
    }
    qb.push(")");
    qb.build().execute(pool).await?;
    Ok(())
}

/// invariant: G-REC-002 — touch heartbeat. Called from chunk upload handler.
pub async fn touch_last_chunk(pool: &SqlitePool, recording_id: &str) -> Result<()> {
    sqlx::query("UPDATE recordings SET last_chunk_at = datetime('now') WHERE id = ?")
        .bind(recording_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Mark `finalizing` while the finalize task is running. Idempotent guard
/// against duplicate finalize (G-TASK-002 — caller still checks current state).
pub async fn mark_finalizing(pool: &SqlitePool, recording_id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE recordings SET format = 'finalizing' WHERE id = ? AND format = 'recording'",
    )
    .bind(recording_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_finalized(
    pool: &SqlitePool,
    recording_id: &str,
    file_path: &str,
    original_filename: &str,
    format: &str,
    duration: Option<f64>,
) -> Result<()> {
    sqlx::query(
        "UPDATE recordings \
         SET file_path = ?, original_filename = ?, format = ?, duration = ?, finalized_at = datetime('now') \
         WHERE id = ?",
    )
    .bind(file_path)
    .bind(original_filename)
    .bind(format)
    .bind(duration)
    .bind(recording_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// G-REC-011 — finalize failure path. Chunks remain on disk for manual recovery.
pub async fn mark_failed(pool: &SqlitePool, recording_id: &str) -> Result<()> {
    sqlx::query("UPDATE recordings SET format = 'failed' WHERE id = ?")
        .bind(recording_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<()> {
    let res = sqlx::query("DELETE FROM recordings WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound(format!("recording {id}")));
    }
    Ok(())
}

/// G-REC-002 + F-REC-007 — at app startup, identify recordings stuck in
/// `recording` state whose last_chunk_at is older than the threshold. Caller
/// auto-finalizes them.
pub async fn list_orphans(pool: &SqlitePool, older_than_seconds: i64) -> Result<Vec<Recording>> {
    Ok(sqlx::query_as::<_, Recording>(
        "SELECT * FROM recordings \
         WHERE format = 'recording' \
           AND (last_chunk_at IS NULL OR (strftime('%s','now') - strftime('%s', last_chunk_at)) > ?)",
    )
    .bind(older_than_seconds)
    .fetch_all(pool)
    .await?)
}
