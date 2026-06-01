//! transcripts repository.
//!
//! invariant: G-TASK-005 — transcript content (`raw_path`/`corrected_path`/
//! `status`) is mutated exclusively by the transcribe task in Phase 2. Other
//! code paths read-only. Callers outside `crate::worker::transcribe` MUST NOT
//! invoke `update_status` / `set_paths` here.

// Phase 2: this whole module is consumed by the (not-yet-built) transcribe
// worker. Allow dead_code on the forward-declared CRUD until Phase 2 wires it.
#![allow(dead_code)]

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::models::Transcript;

/// G-TASK-001 — create row with pre-allocated task_id + status='processing' in
/// a single commit, so the spawned task always finds its own row.
///
/// Caller should:
///   1. let task_id = Uuid::new_v4().to_string();
///   2. let transcript = transcripts::create_processing(pool, note_id, recording_id, &task_id).await?;
///   3. tokio::spawn(transcribe::run(state.clone(), task_id, transcript.id));
pub async fn create_processing(
    pool: &SqlitePool,
    note_id: &str,
    recording_id: Option<&str>,
    task_id: &str,
) -> Result<Transcript> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO transcripts (id, note_id, recording_id, status, task_id) \
         VALUES (?, ?, ?, 'processing', ?)",
    )
    .bind(&id)
    .bind(note_id)
    .bind(recording_id)
    .bind(task_id)
    .execute(pool)
    .await?;
    get(pool, &id).await
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Transcript> {
    sqlx::query_as::<_, Transcript>("SELECT * FROM transcripts WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("transcript {id}")))
}

pub async fn list_for_note(pool: &SqlitePool, note_id: &str) -> Result<Vec<Transcript>> {
    Ok(sqlx::query_as::<_, Transcript>(
        "SELECT * FROM transcripts WHERE note_id = ? ORDER BY created_at ASC",
    )
    .bind(note_id)
    .fetch_all(pool)
    .await?)
}

/// Transcribe-task-only. See module doc.
pub async fn set_paths_and_complete(
    pool: &SqlitePool,
    id: &str,
    raw_path: &str,
    corrected_path: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "UPDATE transcripts SET raw_path = ?, corrected_path = ?, status = 'completed' WHERE id = ?",
    )
    .bind(raw_path)
    .bind(corrected_path)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Transcribe-task-only — failure or cancellation status (G-CANCEL-005).
pub async fn mark_status(pool: &SqlitePool, id: &str, status: &str) -> Result<()> {
    sqlx::query("UPDATE transcripts SET status = ? WHERE id = ?")
        .bind(status)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<()> {
    let res = sqlx::query("DELETE FROM transcripts WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound(format!("transcript {id}")));
    }
    Ok(())
}
