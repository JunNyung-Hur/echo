//! Tool dispatch handlers (Phase 3) — ports chat_agent/agent.py `_execute_tool`
//! + tools.py handlers, mapped onto the new note/note_body domain.
//!
//! Each handler returns a JSON `{ "ok": bool, ... }` result the agent feeds
//! back to the LLM as the tool message. ok=false carries the reason verbatim.

#![allow(dead_code)] // Phase 3: invoked by the agent loop (in progress).

use serde_json::{json, Value};
use tauri::AppHandle;

use crate::chat::refine;
use crate::db::DbPool;
use crate::repo::notes::UpdateNoteInput;
use crate::repo::{ai_endpoints, note_bodies, notes, recordings, transcripts};
use crate::worker::{generate, transcribe};

// 1f207ab — preview only. The chat message lands in the LLM context on every
// subsequent turn, so keep it tiny regardless of transcript length; the full
// text is fetched on demand by the frontend TranscriptViewerModal.
const TRANSCRIPT_PREVIEW_CHARS: usize = 1000;

pub async fn execute_tool(
    app: &AppHandle,
    pool: &DbPool,
    note_id: &str,
    name: &str,
    args: &Value,
) -> Value {
    match name {
        "update_meeting_metadata" => update_metadata(pool, note_id, args).await,
        "refine_minutes" => refine_minutes(app, pool, note_id, args).await,
        "write_note" => write_note_handler(app, pool, note_id, args).await,
        "get_recording_download_url" => recording_path(pool, note_id).await,
        "read_transcript" => read_transcript(pool, note_id).await,
        "retry_transcribe" => retry_transcribe(app, pool, note_id).await,
        "retry_failed_task" => retry_failed_task(app, pool, note_id).await,
        other => json!({ "ok": false, "error": format!("알 수 없는 도구: {other}") }),
    }
}

async fn update_metadata(pool: &DbPool, note_id: &str, args: &Value) -> Value {
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    // location: empty string is an explicit "clear" (Some(None)).
    let location = args.get("location").and_then(|v| v.as_str()).map(|s| {
        if s.is_empty() {
            None
        } else {
            Some(s.to_string())
        }
    });
    let language = args
        .get("language")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    let started_at = args
        .get("started_at")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| Some(s.to_string()));

    let input = UpdateNoteInput {
        title: title.map(String::from),
        location,
        language: language.map(String::from),
        started_at,
        ..Default::default()
    };

    match notes::update(pool, note_id, input).await {
        Ok(_) => {
            // G-CHAT-011 — after a done-stage meta change, prompt the user to
            // reflect it in the body.
            let done = matches!(note_bodies::get_active(pool, note_id).await, Ok(Some(_)));
            let mut r = json!({ "ok": true });
            if done {
                r["hint"] = json!("노트 갱신 여부 되묻기");
            }
            r
        }
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    }
}

async fn refine_minutes(app: &AppHandle, pool: &DbPool, note_id: &str, args: &Value) -> Value {
    let user_request = args
        .get("user_request")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if user_request.is_empty() {
        return json!({ "ok": false, "error": "다듬을 내용(user_request)이 비어 있습니다." });
    }
    match refine::run_refine(app, pool, note_id, user_request).await {
        Ok(body_id) => json!({ "ok": true, "minutes_id": body_id, "status": "completed" }),
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    }
}

/// write_note — 노트 필기형 본문 작성/수정 (refine::run_write).
async fn write_note_handler(app: &AppHandle, pool: &DbPool, note_id: &str, args: &Value) -> Value {
    let user_request = args
        .get("user_request")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if user_request.is_empty() {
        return json!({ "ok": false, "error": "노트에 반영할 내용(user_request)이 비어 있습니다." });
    }
    let intent = args
        .get("intent")
        .and_then(|v| v.as_str())
        .unwrap_or("append");
    match refine::run_write(app, pool, note_id, user_request, intent).await {
        Ok(body_id) => json!({ "ok": true, "note_body_id": body_id, "status": "completed" }),
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    }
}

async fn recording_path(pool: &DbPool, note_id: &str) -> Value {
    match recordings::list_for_note(pool, note_id).await {
        Ok(recs) => match recs.into_iter().find(|r| r.format == "webm") {
            Some(r) => json!({ "ok": true, "file_path": r.file_path }),
            None => json!({ "ok": false, "error": "다운로드할 녹음 파일이 없습니다." }),
        },
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    }
}

async fn read_transcript(pool: &DbPool, note_id: &str) -> Value {
    let transcripts = match transcripts::list_for_note(pool, note_id).await {
        Ok(t) => t,
        Err(e) => return json!({ "ok": false, "error": e.to_string() }),
    };
    let completed = transcripts.iter().find(|t| t.status == "completed");
    let Some(t) = completed else {
        return json!({ "ok": false, "error": "완료된 전사록이 없습니다." });
    };
    let Some(path) = t.corrected_path.clone().or_else(|| t.raw_path.clone()) else {
        return json!({ "ok": false, "error": "전사록 파일 경로가 없습니다." });
    };
    match tokio::fs::read_to_string(&path).await {
        Ok(text) => {
            // 1f207ab — 1K preview only; the full text is fetched on demand by
            // the frontend TranscriptViewerModal via transcript_id. The fence
            // info string `transcript-{id}` is how the chat surface locates the
            // source transcript for the 전체보기 button.
            let total_chars = text.chars().count();
            let preview: String = text.chars().take(TRANSCRIPT_PREVIEW_CHARS).collect();
            let preview_chars = preview.chars().count();
            json!({
                "ok": true,
                "content": format!("```transcript-{}\n{}\n```", t.id, preview),
                "transcript_id": t.id,
                "preview_chars": preview_chars,
                "total_chars": total_chars,
            })
        }
        Err(e) => json!({ "ok": false, "error": format!("전사록을 읽을 수 없습니다: {e}") }),
    }
}

/// Re-run transcription from scratch (G-CHAT: 비파괴 효과를 사용자가 확인한 뒤에만
/// LLM 이 호출). Cleans prior transcripts + bodies, re-dispatches the chain.
async fn retry_transcribe(app: &AppHandle, pool: &DbPool, note_id: &str) -> Value {
    let recs = match recordings::list_for_note(pool, note_id).await {
        Ok(r) => r,
        Err(e) => return json!({ "ok": false, "error": e.to_string() }),
    };
    let Some(rec) = recs.into_iter().find(|r| r.format == "webm") else {
        return json!({ "ok": false, "error": "정리된 녹음이 없어 전사를 재시도할 수 없습니다." });
    };
    if let Ok(bodies) = note_bodies::list_for_note(pool, note_id).await {
        for b in bodies {
            let _ = note_bodies::delete(pool, &b.id).await;
        }
    }
    if let Ok(ts) = transcripts::list_for_note(pool, note_id).await {
        for t in ts {
            let _ = transcripts::delete(pool, &t.id).await;
        }
    }
    match transcribe::dispatch(app, pool, note_id, Some(&rec.id)).await {
        Ok(()) => json!({ "ok": true, "retried": "transcript", "eta_minutes": "5-10" }),
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    }
}

/// Restart only the failed stage: a failed body → re-generate from the existing
/// transcript (fast); a failed transcript → full re-transcribe.
async fn retry_failed_task(app: &AppHandle, pool: &DbPool, note_id: &str) -> Value {
    // Failed body with a completed transcript → re-generate only.
    let bodies = note_bodies::list_for_note(pool, note_id)
        .await
        .unwrap_or_default();
    let ts = transcripts::list_for_note(pool, note_id)
        .await
        .unwrap_or_default();

    let failed_body = bodies
        .iter()
        .find(|b| b.archived == 0 && b.status == "failed");
    let completed_t = ts.iter().find(|t| t.status == "completed");
    if let (Some(b), Some(t)) = (failed_body, completed_t) {
        let Some(path) = t.corrected_path.clone().or_else(|| t.raw_path.clone()) else {
            return json!({ "ok": false, "error": "전사록 파일 경로가 없습니다." });
        };
        if ai_endpoints::get_active(pool, "llm")
            .await
            .ok()
            .flatten()
            .is_none()
        {
            return json!({ "ok": false, "error": "활성 LLM endpoint가 없습니다. 설정에서 등록하세요." });
        }
        let _ = note_bodies::delete(pool, &b.id).await;
        return match generate::dispatch(app, pool, note_id, &t.id, &path).await {
            Ok(()) => json!({ "ok": true, "retried": "minutes", "eta_minutes": "1-2" }),
            Err(e) => json!({ "ok": false, "error": e.to_string() }),
        };
    }

    // Failed transcript → full re-transcribe.
    if ts
        .iter()
        .any(|t| t.status == "failed" || t.status == "cancelled")
    {
        return retry_transcribe(app, pool, note_id).await;
    }

    json!({ "ok": false, "error": "재시작할 실패 작업이 없습니다." })
}
