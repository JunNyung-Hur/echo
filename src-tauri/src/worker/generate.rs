//! Minutes-body generation worker (Phase 2) — ports the stage-1 path of
//! worker/app/tasks/generate.py (`generate_minutes`).
//!
//! Chain position: transcribe → **generate** (G-TASK-003). Stage 1 only;
//! Stage 2 (refine) is Phase 3.
//!
//! Guards:
//!   - G-TASK-001  note_body row committed (status='processing', task_id +
//!     context_snapshot) by `dispatch` before the worker spawns.
//!   - G-TASK-004  one-line summary fills note.description once, never overwrites.
//!   - G-TASK-006  stage-1 auto-call happens exactly once (one note_body/transcript).
//!   - G-TASK-007  initial_* baseline captured once (in note_bodies repo).
//!   - G-TASK-009  LLM retry handled in `ai::chat_completion`.
//!   - G-CANCEL-005  on cancel → status='cancelled' + timeline event.
//!   - G-TASK-010  whole task wrapped in TASK_TIME_LIMIT.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::ai;
use crate::error::{Error, Result};
use crate::models::Note;
use crate::prompts;
use crate::repo::{ai_endpoints, note_bodies, notes};
use crate::timeline;
use crate::worker::{check_cancelled, TASK_TIME_LIMIT};
use crate::AppState;

pub(crate) fn build_meeting_info(note: &Note) -> String {
    let mut s = String::new();
    // 기본 제목("제목 없음"/"Untitled")은 넘기지 않는다 — 그러면 LLM이 그걸 h1 제목으로
    // 그대로 박아 "제목 없음" 회의록이 된다. 제목이 비면 LLM이 내용에서 직접 짓게 한다.
    let t = note.title.trim();
    if !t.is_empty() && t != "제목 없음" && t != "Untitled" {
        s.push_str(&format!("Title: {t}\n"));
    }
    if let Some(started) = &note.started_at {
        // 날짜만(시각·UTC 제거) — 회의록 본문에 날짜 정도는 허용하되 시각은 노출 안 함.
        let date_only = started.split('T').next().unwrap_or(started);
        if !date_only.is_empty() {
            s.push_str(&format!("Date: {date_only}\n"));
        }
    }
    if let Some(loc) = &note.location {
        if !loc.is_empty() {
            s.push_str(&format!("Location: {}\n", loc));
        }
    }
    s.push_str(&format!("Language: {}\n", note.language));
    if let Some(desc) = &note.description {
        if !desc.is_empty() {
            s.push_str(&format!("Memo: {}\n", desc));
        }
    }
    s
}

/// G-DB-001 — context_snapshot is NOT NULL + valid JSON.
pub(crate) fn context_snapshot_json(note: &Note) -> String {
    serde_json::json!({
        "title": note.title,
        "description": note.description,
        "location": note.location,
        "language": note.language,
        "started_at": note.started_at,
    })
    .to_string()
}

/// G-TASK-001 — create a `processing` note_body row (pre-allocated task_id +
/// context_snapshot) in a single commit, then spawn the worker.
pub async fn dispatch(
    app: &AppHandle,
    pool: &SqlitePool,
    note_id: &str,
    transcript_id: &str,
    transcript_path: &str,
) -> Result<()> {
    let note = notes::get(pool, note_id).await?;
    let ctx = context_snapshot_json(&note);
    let task_id = Uuid::new_v4().to_string();
    let body =
        note_bodies::create_processing(pool, note_id, Some(transcript_id), &task_id, &ctx).await?;
    spawn(app.clone(), body.id, task_id, transcript_path.to_string());
    Ok(())
}

pub fn spawn(app: AppHandle, body_id: String, task_id: String, transcript_path: String) {
    let (pool, flag) = match app.try_state::<AppState>() {
        Some(s) => (s.db.clone(), s.cancellations.register(task_id.clone())),
        None => return,
    };

    tauri::async_runtime::spawn(async move {
        let outcome = tokio::time::timeout(
            TASK_TIME_LIMIT,
            run(
                app.clone(),
                pool.clone(),
                body_id.clone(),
                transcript_path,
                flag,
            ),
        )
        .await;

        match outcome {
            Ok(Ok(())) => {}
            Ok(Err(Error::Cancelled)) => {
                finish(
                    &pool,
                    &body_id,
                    "cancelled",
                    "minutes_cancelled",
                    "노트 정리가 취소되었습니다.",
                )
                .await;
            }
            Ok(Err(e)) => {
                tracing::error!(?e, %body_id, "generate failed");
                finish(
                    &pool,
                    &body_id,
                    "failed",
                    "minutes_failed",
                    "노트 정리 중 오류가 발생했습니다.",
                )
                .await;
            }
            Err(_) => {
                tracing::error!(%body_id, "generate timed out");
                finish(
                    &pool,
                    &body_id,
                    "failed",
                    "minutes_failed",
                    "노트 정리 시간 초과로 중단되었습니다.",
                )
                .await;
            }
        }

        if let Some(s) = app.try_state::<AppState>() {
            s.cancellations.unregister(&task_id);
        }
        let _ = app.emit("note:updated", body_id.clone());
    });
}

async fn finish(pool: &SqlitePool, body_id: &str, status: &str, kind: &str, msg: &str) {
    if let Ok(b) = note_bodies::get(pool, body_id).await {
        if b.status == "completed" {
            return;
        }
        let _ = note_bodies::mark_status(pool, body_id, status).await;
        let _ = timeline::post(pool, &b.note_id, kind, msg).await;
    }
}

async fn run(
    app: AppHandle,
    pool: SqlitePool,
    body_id: String,
    transcript_path: String,
    flag: Arc<AtomicBool>,
) -> Result<()> {
    check_cancelled(&flag)?;

    let body = note_bodies::get(&pool, &body_id).await?;
    let note = notes::get(&pool, &body.note_id).await?;
    tracing::info!(%body_id, note_id = %body.note_id, "generate: started");
    let _ = timeline::post(
        &pool,
        &body.note_id,
        "minutes_started",
        "노트 정리가 시작되었습니다.",
    )
    .await;
    // Meetzy minutes step — fill the 95-100% band of the transcribing monitor
    // while the body is generated. generate isn't chunked, so there's no inner
    // progress; the bar sits at the step midpoint (~98%) until done flips the
    // panel over. (transcribe:progress is the channel the monitor already
    // listens on; stage="minutes" routes it to the 3rd step.)
    let _ = app.emit(
        "transcribe:progress",
        serde_json::json!({
            "note_id": body.note_id,
            "current": 0,
            "total": 1,
            "stage": "minutes",
        }),
    );

    let llm_ep = ai_endpoints::get_active(&pool, "llm")
        .await?
        .ok_or_else(|| Error::Other("활성 LLM endpoint가 없습니다. 설정에서 등록하세요.".into()))?;

    let transcript_text =
        tokio::fs::read_to_string(crate::storage::resolve(&transcript_path)).await?;
    let meeting_info = build_meeting_info(&note);

    // e3d01f5 — 노트 출력 언어 = ui_lang(설정). 전사 언어와 달라도 이 언어로 작성(번역).
    let ui_lang = crate::repo::settings::get(&pool, "ui_lang").await.ok().flatten();
    let target_lang = if ui_lang.as_deref() == Some("en") { "en" } else { "ko" };

    let mut user_content =
        format!("[Meeting Info]\n{meeting_info}\n\n[Transcript]\n{transcript_text}");
    if target_lang == "en" {
        // 전사(한국어일 수 있음) 뒤에 출력 언어를 recency로 재확인. KO는 안 건드림.
        user_content.push_str(
            "\n\n[OUTPUT LANGUAGE — IMPORTANT]\n\
             Write the entire minutes in ENGLISH. The transcript above is likely Korean; \
             translate its content into natural English. Do NOT output Korean sentences.",
        );
    }

    let system_content = prompts::minutes_system_prompt(target_lang);
    let result = ai::chat_completion(&llm_ep, &system_content, &user_content).await?;
    let minutes_html = result.content;
    if minutes_html.trim().is_empty() {
        return Err(Error::Other("LLM produced empty minutes".into()));
    }

    // Persist content + complete (G-TASK-007 initial capture handled in repo).
    // Note-centric storage — body lives under the note's folder; store the
    // app_data-relative path.
    let content_rel = crate::storage::body_rel(&body.note_id, &body_id);
    let content_path = crate::storage::resolve(&content_rel);
    if let Some(parent) = content_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&content_path, minutes_html.as_bytes()).await?;
    note_bodies::set_content_and_complete(&pool, &body_id, &content_rel).await?;
    // 제목=본문 첫 줄 (freeform write_note와 동일 규칙). minutes도 생성된 본문에서 제목을 도출해
    // notes.title을 갱신한다 — 안 하면 "(제목 없음)" 기본값이 그대로 남는다.
    let _ = notes::update(
        &pool,
        &body.note_id,
        notes::UpdateNoteInput {
            title: Some(crate::chat::refine::extract_title(&minutes_html)),
            ..Default::default()
        },
    )
    .await;
    tracing::info!(%body_id, chars = minutes_html.len(), "generate: completed");
    let _ = timeline::post(
        &pool,
        &body.note_id,
        "minutes_generated",
        "노트 정리가 완료되었습니다.",
    )
    .await;
    // F-DESKTOP-004 — final completion ping (app may be tray-minimized).
    crate::worker::notify(&app, &pool, "notify_note", "노트 준비 완료", &format!("{} — 본문이 준비됐어요", note.title)).await;

    // G-TASK-004 — one-line summary fills note.description once (never overwrite).
    check_cancelled(&flag)?;
    let needs_summary = note
        .description
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty();
    if needs_summary {
        if let Ok(r) = ai::chat_completion(
            &llm_ep,
            prompts::MINUTES_ONE_LINE_SUMMARY_PROMPT,
            &format!("[Minutes]\n{minutes_html}"),
        )
        .await
        {
            let summary: String = r
                .content
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("")
                .chars()
                .take(120)
                .collect();
            if !summary.is_empty() {
                // Re-check emptiness right before write to avoid clobbering a
                // value the user set in the meantime.
                if let Ok(fresh) = notes::get(&pool, &body.note_id).await {
                    if fresh
                        .description
                        .as_deref()
                        .map(str::trim)
                        .unwrap_or("")
                        .is_empty()
                    {
                        let _ = notes::update(
                            &pool,
                            &body.note_id,
                            notes::UpdateNoteInput {
                                description: Some(Some(summary)),
                                ..Default::default()
                            },
                        )
                        .await;
                    }
                }
            }
        }
    }

    Ok(())
}
