//! Long-running tokio tasks — replaces the old Celery worker.
//!
//! Each sub-module owns one stage of the chain (G-TASK-* in
//! migration/guards-and-invariants.md):
//!
//!   finalize  →  transcribe  →  generate  →  (user-driven) refine
//!
//! Phase 1 only ships `finalize`. Phase 2 wires `transcribe` + `generate`.
//!
//! All task spawns follow the **G-TASK-001 single-commit pattern**:
//!   1. Pre-generate a TaskId (UUID v4).
//!   2. Insert / update the owning DB row (status='processing', task_id=…) in
//!      a single transaction.
//!   3. Register a cancellation flag in `AppState.cancellations`.
//!   4. `tauri::async_runtime::spawn` the worker future.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::error::{Error, Result};

pub mod finalize;
pub mod generate;
pub mod transcribe;

/// F-DESKTOP-004 — best-effort OS notification on task completion, so the user
/// can leave the app in the tray and still get pinged when transcribe/minutes
/// finish. Any failure (e.g. permission denied) is logged and swallowed — a
/// notification must never block or fail the chain.
///
/// `show()` is a synchronous OS/COM call (winrt toast on Windows) that can take
/// a beat; run it on a blocking thread so it never stalls the worker's async
/// runtime thread (which would delay the next chain step + make the toast feel
/// laggy relative to completion).
pub async fn notify(app: &AppHandle, pool: &sqlx::SqlitePool, pref_key: &str, title: &str, body: &str) {
    // 사용자가 끈 알림("0")은 보내지 않는다 — 설정이 없으면 기본 on.
    if matches!(crate::repo::settings::get(pool, pref_key).await, Ok(Some(v)) if v == "0") {
        return;
    }
    let app = app.clone();
    let title = title.to_string();
    let body = body.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = app.notification().builder().title(title).body(body).show() {
            tracing::warn!(?e, "notification failed");
        }
    });
}

/// G-TASK-010 — hard ceiling on any single worker task (transcribe/generate),
/// enforced by wrapping the worker future in `tokio::time::timeout`.
pub const TASK_TIME_LIMIT: Duration = Duration::from_secs(30 * 60);

/// Poll a cancellation flag at a checkpoint — G-CANCEL-002 (before each ASR
/// chunk), G-CANCEL-004 (before chaining to the next stage). Returns
/// `Err(Error::Cancelled)` so the worker unwinds and its wrapper marks the row
/// `status='cancelled'` + posts a timeline event (G-CANCEL-005).
pub fn check_cancelled(flag: &AtomicBool) -> Result<()> {
    if flag.load(Ordering::SeqCst) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
