//! note_bodies repository (old: minutes — D-015).
//!
//! invariant: G-DB-001 — context_snapshot is NOT NULL + valid JSON (DB-level).
//! invariant: G-DB-004 — at most one active (archived=0, completed) body/note.
//! invariant: G-TASK-007 — initial_* baseline captured exactly once at first
//! completion, then carried forward across versions.
//! invariant: G-TASK-005 analogue — content (`content_path`/`status`) is
//! mutated exclusively by the generate/refine task in Phase 2.

// Phase 2: consumed by the (not-yet-built) generate worker. Allow dead_code on
// these forward-declared helpers until Phase 2 wires them up.
#![allow(dead_code)]

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::models::NoteBody;

/// G-TASK-001 — create a `processing` row with a pre-allocated task_id +
/// context_snapshot in a single commit, so the spawned generate task always
/// finds its own row.
///
/// Caller should:
///   1. let task_id = Uuid::new_v4().to_string();
///   2. let body = note_bodies::create_processing(pool, note_id, tid, &task_id, &ctx).await?;
///   3. worker::generate::spawn(handle, task_id, body.id);
pub async fn create_processing(
    pool: &SqlitePool,
    note_id: &str,
    transcript_id: Option<&str>,
    task_id: &str,
    context_snapshot_json: &str,
) -> Result<NoteBody> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO note_bodies (id, note_id, transcript_id, status, task_id, context_snapshot) \
         VALUES (?, ?, ?, 'processing', ?, ?)",
    )
    .bind(&id)
    .bind(note_id)
    .bind(transcript_id)
    .bind(task_id)
    .bind(context_snapshot_json)
    .execute(pool)
    .await?;
    get(pool, &id).await
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<NoteBody> {
    sqlx::query_as::<_, NoteBody>("SELECT * FROM note_bodies WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("note_body {id}")))
}

/// The single active body for a note (G-DB-004), if any.
pub async fn get_active(pool: &SqlitePool, note_id: &str) -> Result<Option<NoteBody>> {
    Ok(sqlx::query_as::<_, NoteBody>(
        "SELECT * FROM note_bodies \
         WHERE note_id = ? AND archived = 0 AND status = 'completed' LIMIT 1",
    )
    .bind(note_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn list_for_note(pool: &SqlitePool, note_id: &str) -> Result<Vec<NoteBody>> {
    Ok(sqlx::query_as::<_, NoteBody>(
        "SELECT * FROM note_bodies WHERE note_id = ? ORDER BY created_at ASC",
    )
    .bind(note_id)
    .fetch_all(pool)
    .await?)
}

/// Mark a body completed with its content path.
///
/// invariant: G-TASK-007 — capture the initial_* baseline exactly once. COALESCE
/// keeps the first values on any later completion (e.g. retry) so the user's
/// "최초 상태로 되돌리기" target stays the original stage-1 output.
pub async fn set_content_and_complete(
    pool: &SqlitePool,
    id: &str,
    content_path: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE note_bodies SET \
            content_path = ?, \
            status = 'completed', \
            initial_content_path = COALESCE(initial_content_path, ?), \
            initial_context_snapshot = COALESCE(initial_context_snapshot, context_snapshot) \
         WHERE id = ?",
    )
    .bind(content_path)
    .bind(content_path)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// G-REFINE-003 / G-VERSION-002 — archive the current active body and create a
/// new completed one in one transaction (G-DB-004 one-active never sees two).
/// `initial_*` carries the stage-1 baseline forward (G-VERSION-004 / G-TASK-007).
#[allow(clippy::too_many_arguments)]
pub async fn archive_and_create_completed(
    pool: &SqlitePool,
    id: &str,
    note_id: &str,
    transcript_id: Option<&str>,
    content_path: &str,
    context_snapshot_json: &str,
    initial_content_path: Option<&str>,
    initial_context_snapshot: Option<&str>,
    is_manual: bool,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE note_bodies SET archived = 1 WHERE note_id = ? AND archived = 0")
        .bind(note_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO note_bodies \
         (id, note_id, transcript_id, content_path, status, context_snapshot, initial_content_path, initial_context_snapshot, is_manual_edit) \
         VALUES (?, ?, ?, ?, 'completed', ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(note_id)
    .bind(transcript_id)
    .bind(content_path)
    .bind(context_snapshot_json)
    .bind(initial_content_path)
    .bind(initial_context_snapshot)
    .bind(is_manual as i64)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Generate/refine-task-only — failure or cancellation status (G-CANCEL-005).
pub async fn mark_status(pool: &SqlitePool, id: &str, status: &str) -> Result<()> {
    sqlx::query("UPDATE note_bodies SET status = ? WHERE id = ?")
        .bind(status)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<()> {
    let res = sqlx::query("DELETE FROM note_bodies WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound(format!("note_body {id}")));
    }
    Ok(())
}
