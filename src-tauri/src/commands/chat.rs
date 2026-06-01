//! Chat agent commands (Phase 3).

use serde_json::Value;
use tauri::{AppHandle, State};

use crate::error::Result;
use crate::models::ChatMessage;
use crate::repo::chat as chat_repo;
use crate::AppState;

/// Send one user message; runs the agent loop (may take 1-2min if it refines).
/// Emits `chat:status` / `chat:done` events during; the frontend reloads
/// messages on resolution.
#[tauri::command]
pub async fn chat_send(
    app: AppHandle,
    state: State<'_, AppState>,
    note_id: String,
    message: String,
    user_state: Option<Value>,
) -> Result<()> {
    let pool = state.db.clone();
    // 첨부 녹음의 consumed 처리·메시지 연결은 run_agent가 유저 메시지를 만든 직후
    // 수행한다(메시지 id가 필요하므로). 여기선 그대로 위임.
    crate::chat::agent::run_agent(&app, &pool, &note_id, &message, user_state).await
}

#[tauri::command]
pub async fn list_chat_messages(
    state: State<'_, AppState>,
    note_id: String,
) -> Result<Vec<ChatMessage>> {
    chat_repo::list_for_note(&state.db, &note_id).await
}
