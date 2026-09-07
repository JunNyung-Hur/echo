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
    // 진행 중 표시 등록 — 노트를 나갔다 돌아와도 chat_running으로 "응답 대기"
    // 인디케이터를 복원한다. 어떤 경로로 끝나든 반드시 해제(끝에서 remove).
    if !state.chat_runs.insert(note_id.clone()) {
        return Err(crate::error::Error::Other(
            "A turn is already running for this note.".into(),
        ));
    }
    // 첨부 녹음의 consumed 처리·메시지 연결은 run_agent가 유저 메시지를 만든 직후
    // 수행한다(메시지 id가 필요하므로). 여기선 그대로 위임.
    let result = crate::chat::agent::run_agent(&app, &pool, &note_id, &message, user_state).await;
    state.chat_runs.remove(&note_id);
    result
}

/// 이 노트의 에이전트 턴이 지금 진행 중인가 — 노트 재진입 시 pending 인디케이터 복원용.
#[tauri::command]
pub async fn chat_running(state: State<'_, AppState>, note_id: String) -> Result<bool> {
    Ok(state.chat_runs.contains(&note_id))
}

#[tauri::command]
pub async fn list_chat_messages(
    state: State<'_, AppState>,
    note_id: String,
) -> Result<Vec<ChatMessage>> {
    chat_repo::list_for_note(&state.db, &note_id).await
}
