//! Reading an empty notebook is valid; a missing notebook or broken file is not.
use serde_json::{json, Value};

use crate::{db::DbPool, repo::note_bodies};

pub async fn read(pool: &DbPool, note_id: &str) -> Value {
    match read_inner(pool, note_id).await {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "error": error.to_string()}),
    }
}

async fn read_inner(pool: &DbPool, note_id: &str) -> crate::error::Result<Value> {
    let note: Option<(Option<String>,)> =
        sqlx::query_as("SELECT note_type FROM notes WHERE id = ?")
            .bind(note_id)
            .fetch_optional(pool)
            .await?;
    let Some((note_type,)) = note else {
        return Ok(json!({"ok": false, "error": "Note not found"}));
    };
    if let Some(body) = note_bodies::get_active(pool, note_id).await? {
        let Some(path) = body.content_path else {
            return Ok(json!({"ok": false, "error": "Note body file path missing"}));
        };
        let content = tokio::fs::read_to_string(crate::storage::resolve(&path)).await?;
        return Ok(json!({"ok": true, "content": content, "version_id": body.id,
            "body_state": "ready"}));
    }
    let bodies = note_bodies::list_for_note(pool, note_id).await?;
    if bodies.iter().any(|b| b.archived == 0 && matches!(b.status.as_str(), "pending" | "processing")) {
        return Ok(json!({"ok": false, "body_state": "processing",
            "error": "Note generation is still in progress"}));
    }
    if note_type.as_deref() == Some("freeform") {
        return Ok(json!({"ok": true, "content": "", "version_id": null,
            "body_state": "empty", "can_write": true,
            "hint": "The current notebook exists and is empty. If the user wants content saved, use write_note without after/base_version to create its first body. No title or additional permission is required. Questions do not require creating a body."}));
    }
    Ok(json!({"ok": false, "body_state": "unavailable",
        "error": "No completed note body is available"}))
}
