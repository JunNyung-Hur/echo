//! tags + note_tags repository (Phase 4 — F-TAG / G-TAG).
//!
//! tags carry a NOCASE-unique name (G-TAG-001, enforced by idx_tags_name).
//! note_tags is the M2M join; both FKs are ON DELETE CASCADE (G-TAG-002), so
//! deleting a tag drops its links and deleting a note drops its tag rows.

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::Result;
use crate::models::{Tag, TagWithCount};

/// Tags used by at least one note (usage > 0), name-sorted — drives the home
/// filter. INNER JOIN drops orphan tags so a tag removed from its last note no
/// longer shows up as a "name 0" chip.
pub async fn list(pool: &SqlitePool) -> Result<Vec<TagWithCount>> {
    Ok(sqlx::query_as::<_, TagWithCount>(
        "SELECT t.id, t.name, t.color, t.created_at, \
            COUNT(nt.note_id) AS usage \
         FROM tags t JOIN note_tags nt ON nt.tag_id = t.id \
         GROUP BY t.id ORDER BY t.name COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?)
}

/// Tags attached to one note, in the order they were added (nt.rowid) so a
/// newly added tag appends at the end instead of jumping by name.
pub async fn for_note(pool: &SqlitePool, note_id: &str) -> Result<Vec<Tag>> {
    Ok(sqlx::query_as::<_, Tag>(
        "SELECT t.id, t.name, t.color, t.created_at \
         FROM tags t JOIN note_tags nt ON nt.tag_id = t.id \
         WHERE nt.note_id = ? ORDER BY nt.rowid",
    )
    .bind(note_id)
    .fetch_all(pool)
    .await?)
}

/// Name-prefix autocomplete (F-TAG-003), case-insensitive and capped.
pub async fn suggest(pool: &SqlitePool, prefix: &str, limit: i64) -> Result<Vec<Tag>> {
    // Escape LIKE wildcards so the user's `%`/`_`/`\` match literally.
    let escaped = prefix
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("{escaped}%");
    Ok(sqlx::query_as::<_, Tag>(
        "SELECT id, name, color, created_at FROM tags \
         WHERE name LIKE ? ESCAPE '\\' COLLATE NOCASE \
         ORDER BY name COLLATE NOCASE LIMIT ?",
    )
    .bind(pattern)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Tag> {
    Ok(
        sqlx::query_as::<_, Tag>("SELECT id, name, color, created_at FROM tags WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await?,
    )
}

/// Get an existing tag by name (NOCASE) or create it. Idempotent — the "add
/// tag" path calls this so a duplicate name reuses the row instead of tripping
/// the unique constraint (G-TAG-001).
pub async fn get_or_create(pool: &SqlitePool, name: &str, color: Option<&str>) -> Result<Tag> {
    let name = name.trim();
    if let Some(existing) = sqlx::query_as::<_, Tag>(
        "SELECT id, name, color, created_at FROM tags WHERE name = ? COLLATE NOCASE",
    )
    .bind(name)
    .fetch_optional(pool)
    .await?
    {
        return Ok(existing);
    }
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO tags (id, name, color) VALUES (?, ?, ?)")
        .bind(&id)
        .bind(name)
        .bind(color)
        .execute(pool)
        .await?;
    get(pool, &id).await
}

/// Rename a tag (F-TAG-001 update). NOCASE-unique still applies.
pub async fn rename(pool: &SqlitePool, id: &str, name: &str) -> Result<Tag> {
    sqlx::query("UPDATE tags SET name = ? WHERE id = ?")
        .bind(name.trim())
        .bind(id)
        .execute(pool)
        .await?;
    get(pool, id).await
}

/// Delete a tag. note_tags rows cascade away (G-TAG-002).
pub async fn delete(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM tags WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Attach a tag to a note. Idempotent — the composite PK dedupes.
pub async fn attach(pool: &SqlitePool, note_id: &str, tag_id: &str) -> Result<()> {
    sqlx::query("INSERT OR IGNORE INTO note_tags (note_id, tag_id) VALUES (?, ?)")
        .bind(note_id)
        .bind(tag_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Detach a tag from a note. If that was the tag's last note, the tag row
/// itself is removed so orphaned tags don't linger in the home filter.
pub async fn detach(pool: &SqlitePool, note_id: &str, tag_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM note_tags WHERE note_id = ? AND tag_id = ?")
        .bind(note_id)
        .bind(tag_id)
        .execute(pool)
        .await?;
    // Orphan cleanup — drop the tag once no note carries it anymore.
    sqlx::query("DELETE FROM tags WHERE id = ? AND NOT EXISTS (SELECT 1 FROM note_tags WHERE tag_id = ?)")
        .bind(tag_id)
        .bind(tag_id)
        .execute(pool)
        .await?;
    Ok(())
}
