//! Tauri command handlers — the IPC boundary called from React via
//! `@tauri-apps/api/core invoke()`.
//!
//! Sub-modules own one resource each (notes, recordings, transcripts, …).
//! Top-level here keeps tiny app-wide helpers (version, db ping).

pub mod ai_endpoints;
pub mod audio;
pub mod chat;
pub mod notes;
pub mod processing;
pub mod recordings;
pub mod settings;
pub mod tags;

use crate::error::Result;
use crate::AppState;

#[tauri::command]
pub fn get_app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Sanity check the DB is reachable. Phase 0 verification helper.
#[tauri::command]
pub async fn ping_db(state: tauri::State<'_, AppState>) -> Result<String> {
    let row: (i64,) = sqlx::query_as("SELECT 1").fetch_one(&state.db).await?;
    Ok(format!("ok ({})", row.0))
}

/// OS username — replaces SSO `user.full_name` (D-008) for the time-of-day
/// greeting. Falls back to "사용자" if env not set.
#[tauri::command]
pub fn get_username() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "사용자".to_string())
}
