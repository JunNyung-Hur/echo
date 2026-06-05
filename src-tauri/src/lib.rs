mod ai;
mod asr;
mod audio_capture;
mod audio_volume;
mod cancellation;
mod chat;
mod commands;
mod db;
mod error;
mod ffmpeg;
mod models;
mod prompts;
mod repo;
mod storage;
mod timeline;
mod worker;

#[cfg(test)]
mod e2e_real;

use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Shared application state managed by Tauri. Accessed from commands via
/// `tauri::State<AppState>`.
pub struct AppState {
    pub db: db::DbPool,
    pub cancellations: cancellation::Registry,
    /// Live native capture sessions keyed by recording_id (D-023 / P1R-01).
    pub captures: std::sync::Mutex<std::collections::HashMap<String, audio_capture::CaptureHandle>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // OS user data directory — DB lives here (D-004).
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");
            std::fs::create_dir_all(&app_data_dir)?;
            // Capture app_data_dir for the storage layer (note-centric paths) —
            // must run before any artifact path is resolved.
            storage::init(app_data_dir.clone());
            let db_path = app_data_dir.join("echo.db");

            // Wipe leftover input-test temp files. They're session-scoped and
            // only persist if the app was killed mid-test; nothing here is worth
            // keeping across launches.
            let test_dir = app_data_dir.join("test-captures");
            if test_dir.exists() {
                let _ = std::fs::remove_dir_all(&test_dir);
            }

            tracing::info!(?app_data_dir, ?db_path, "initializing app state");

            // Bridge tauri's sync setup with async DB init.
            let db = tauri::async_runtime::block_on(async {
                db::init_pool(&db_path).await
            })?;

            // Default-select one endpoint per kind if any exist but none active
            // (e.g. registered before auto-activate). Non-fatal.
            let _ = tauri::async_runtime::block_on(repo::ai_endpoints::ensure_default_active(&db));

            app.manage(AppState {
                db,
                cancellations: cancellation::Registry::new(),
                captures: std::sync::Mutex::new(std::collections::HashMap::new()),
            });

            // Tray icon + menu (P1-03).
            #[cfg(desktop)]
            {
                build_tray(app.handle())?;
            }

            // ffmpeg presence check (D-022). Non-blocking — finalize handles
            // absence gracefully (G-REC-011), but log it loud so the user
            // installs before they finish their first recording.
            tauri::async_runtime::spawn(async {
                if !ffmpeg::is_available().await {
                    tracing::warn!(
                        "ffmpeg not found on PATH — finalize will fail until installed (winget install Gyan.FFmpeg)"
                    );
                } else {
                    tracing::info!("ffmpeg available");
                }
            });

            // P1-09 / F-REC-007 / G-REC-002 — orphan recovery.
            //
            // Any recording stuck in format='recording' whose last_chunk_at is
            // older than 60s is assumed to be a victim of an unexpected app
            // close. Auto-flip to 'finalizing' + spawn the finalize task so the
            // user comes back to a usable note instead of a half-state row.
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = match handle.try_state::<AppState>() {
                        Some(s) => s,
                        None => return,
                    };
                    match repo::recordings::list_orphans(&state.db, 60).await {
                        Ok(orphans) => {
                            for orphan in orphans {
                                tracing::info!(
                                    recording_id = %orphan.id,
                                    "orphan recording — auto-finalize"
                                );
                                let _ = repo::recordings::mark_finalizing(&state.db, &orphan.id).await;
                                worker::finalize::spawn(handle.clone(), orphan.id);
                            }
                        }
                        Err(e) => tracing::warn!(?e, "list_orphans failed"),
                    }
                });
            }

            tracing::info!("setup complete");
            Ok(())
        })
        // Prevent main window close from quitting the app — hide to tray instead.
        // F-DESKTOP-005. User can still quit via the tray menu.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_version,
            commands::ping_db,
            commands::get_username,
            commands::notes::create_note,
            commands::notes::list_notes,
            commands::notes::get_note,
            commands::notes::update_note,
            commands::notes::delete_note,
            commands::notes::note_folder_path,
            commands::recordings::list_recordings,
            commands::recordings::list_pending_recordings,
            commands::recordings::list_archived_recordings,
            commands::recordings::get_recording,
            commands::recordings::read_recording_audio,
            commands::recordings::start_recording,
            commands::recordings::stop_recording,
            commands::recordings::delete_recording,
            commands::recordings::import_audio_file,
            commands::ai_endpoints::list_endpoints,
            commands::ai_endpoints::create_endpoint,
            commands::ai_endpoints::update_endpoint,
            commands::ai_endpoints::delete_endpoint,
            commands::ai_endpoints::activate_endpoint,
            commands::ai_endpoints::test_endpoint,
            commands::audio::list_audio_devices,
            commands::audio::get_source_volume,
            commands::audio::set_source_volume,
            commands::audio::start_test_capture,
            commands::audio::stop_test_capture,
            commands::processing::list_transcripts,
            commands::processing::get_transcript_content,
            commands::processing::list_note_bodies,
            commands::processing::list_timeline,
            commands::processing::get_body_content,
            commands::processing::retry_transcribe,
            commands::processing::restore_note_body,
            commands::processing::save_manual_body_edit,
            commands::chat::chat_send,
            commands::chat::list_chat_messages,
            commands::settings::get_setting,
            commands::settings::set_setting,
            commands::tags::list_tags,
            commands::tags::list_note_tags,
            commands::tags::suggest_tags,
            commands::tags::add_note_tag,
            commands::tags::remove_note_tag,
            commands::tags::rename_tag,
            commands::tags::delete_tag,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("echo_app_lib=debug,tauri=info,info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .init();
}

// ============================================================================
// Tray  (P1-03)
// ============================================================================

#[cfg(desktop)]
fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    // Menu items — each `id` is matched in `on_tray_menu_event`.
    let open = MenuItem::with_id(app, "tray-open", "echo 열기", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "tray-quit", "종료", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&open, &sep, &quit])?;

    let _tray = TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("echo")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_tray_menu_event)
        // Left-click on tray icon = bring main window forward.
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg(desktop)]
fn on_tray_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id.as_ref() {
        "tray-open" => show_main_window(app),
        "tray-quit" => {
            tracing::info!("quit requested from tray");
            app.exit(0);
        }
        other => tracing::warn!(id = %other, "unknown tray menu id"),
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
