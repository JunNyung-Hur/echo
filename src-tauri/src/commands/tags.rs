//! Tag commands (Phase 4 — F-TAG). Tag CRUD, per-note attach/detach,
//! autocomplete, and the filter-sidebar list.

use tauri::State;

use crate::error::{Error, Result};
use crate::models::{Tag, TagWithCount};
use crate::repo::tags as repo;
use crate::AppState;

/// All tags + usage counts (F-TAG-004 filter sidebar).
#[tauri::command]
pub async fn list_tags(state: State<'_, AppState>) -> Result<Vec<TagWithCount>> {
    repo::list(&state.db).await
}

/// Tags attached to one note (chips on the note view).
#[tauri::command]
pub async fn list_note_tags(state: State<'_, AppState>, note_id: String) -> Result<Vec<Tag>> {
    repo::for_note(&state.db, &note_id).await
}

/// Name-prefix autocomplete (F-TAG-003).
#[tauri::command]
pub async fn suggest_tags(state: State<'_, AppState>, prefix: String) -> Result<Vec<Tag>> {
    repo::suggest(&state.db, &prefix, 8).await
}

/// Create-or-reuse a tag by name and attach it to a note in one call — the
/// "add tag" path from the note view.
#[tauri::command]
pub async fn add_note_tag(
    state: State<'_, AppState>,
    note_id: String,
    name: String,
) -> Result<Tag> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::InvalidInput("tag name is empty".into()));
    }
    let tag = repo::get_or_create(&state.db, name, None).await?;
    repo::attach(&state.db, &note_id, &tag.id).await?;
    Ok(tag)
}

/// Detach a tag from a note (the tag row itself survives for other notes).
#[tauri::command]
pub async fn remove_note_tag(
    state: State<'_, AppState>,
    note_id: String,
    tag_id: String,
) -> Result<()> {
    repo::detach(&state.db, &note_id, &tag_id).await
}

/// Rename a tag everywhere it's used (F-TAG-001 update).
#[tauri::command]
pub async fn rename_tag(state: State<'_, AppState>, id: String, name: String) -> Result<Tag> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::InvalidInput("tag name is empty".into()));
    }
    repo::rename(&state.db, &id, name).await
}

/// Delete a tag globally; note_tags rows cascade (G-TAG-002).
#[tauri::command]
pub async fn delete_tag(state: State<'_, AppState>, id: String) -> Result<()> {
    repo::delete(&state.db, &id).await
}
