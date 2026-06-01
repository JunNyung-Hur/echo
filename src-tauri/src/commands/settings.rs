//! App settings commands — KV-backed preferences (currently UI language).

use tauri::State;

use crate::error::Result;
use crate::repo::settings as repo;
use crate::AppState;

/// Read a setting's value, or null if the key was never set.
#[tauri::command]
pub async fn get_setting(state: State<'_, AppState>, key: String) -> Result<Option<String>> {
    repo::get(&state.db, &key).await
}

/// Upsert a setting.
#[tauri::command]
pub async fn set_setting(state: State<'_, AppState>, key: String, value: String) -> Result<()> {
    repo::set(&state.db, &key, &value).await
}
