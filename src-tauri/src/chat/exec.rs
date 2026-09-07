//! Tool dispatch handlers — Meetzy d75150c `chat_agent/tools.py` 이식
//! (talker=doer: 편집은 에이전트가 직접 수행하는 동기 툴, 워커 dispatch 없음).
//!
//! Each handler returns a JSON `{ "ok": bool, ... }` result the agent feeds
//! back to the LLM as the tool message. ok=false carries the reason verbatim;
//! `retryable: true` 는 가드 실패 — 모델이 결과를 보고 다음 턴에 자연 재시도.

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::chat::{edit, refine};
use crate::db::DbPool;
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
        "read_minutes" => read_minutes(pool, note_id).await,
        "edit_minutes" => edit_minutes(app, pool, note_id, args).await,
        "set_theme" => set_theme(app, pool, note_id, args).await,
        "write_note" => write_note_handler(app, pool, note_id, args).await,
        "get_recording_download_url" => recording_file(pool, note_id).await,
        "read_transcript" => read_transcript(pool, note_id).await,
        "search_transcripts" | "read_transcript_range" => {
            super::source::execute(pool, note_id, name, args).await
        }
        "retry_transcribe" => retry_transcribe(app, pool, note_id).await,
        "retry_failed_task" => retry_failed_task(app, pool, note_id).await,
        other => json!({ "ok": false, "error": format!("알 수 없는 도구: {other}") }),
    }
}

/// 현재 활성 노트 본문 전체를 조회 — '현재 본문'의 단일 진실 소스(view→edit 패턴).
/// system prompt 에 본문을 박지 않고(스냅샷이 편집 후 낡음) 이 툴로 항상 최신 조회.
async fn read_minutes(pool: &DbPool, note_id: &str) -> Value {
    let active = match note_bodies::get_active(pool, note_id).await {
        Ok(Some(b)) => b,
        Ok(None) => return json!({ "ok": false, "error": "활성 노트가 없음(또는 편집 진행 중)" }),
        Err(e) => return json!({ "ok": false, "error": e.to_string() }),
    };
    let Some(path) = active.content_path.clone() else {
        return json!({ "ok": false, "error": "노트 본문 파일 경로가 없음" });
    };
    match tokio::fs::read_to_string(crate::storage::resolve(&path)).await {
        // 활성 본문 전체 반환(마크다운; 레거시는 HTML일 수 있음) — edit_minutes 의
        // old 매칭과 Q&A 의 단일 진실 소스라 완전해야 한다.
        Ok(content) => json!({ "ok": true, "content": content, "version_id": active.id }),
        Err(e) => json!({ "ok": false, "error": format!("노트 본문 로드 실패: {e}") }),
    }
}

/// 국소 편집 — str_replace 단일 연산(클로드 코드 방식). 가드: 매칭 0/2+·적용된
/// 편집 없음·visible-change 없음(주석만) → retryable 에러 반환(모델이 재시도).
/// 성공 시 새 버전을 *동기* 생성(talker=doer)하고 diff 를 UI 용으로 반환.
async fn edit_minutes(app: &AppHandle, pool: &DbPool, note_id: &str, args: &Value) -> Value {
    let edits: Vec<Value> = args
        .get("edits")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let user_request = args
        .get("user_request")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if edits.is_empty() {
        return json!({ "ok": false, "error": "편집 내용이 비어 있음", "retryable": true });
    }
    let active = match note_bodies::get_active(pool, note_id).await {
        Ok(Some(b)) => b,
        Ok(None) => return json!({ "ok": false, "error": "편집할 활성 노트가 없음" }),
        Err(e) => return json!({ "ok": false, "error": e.to_string() }),
    };
    if args["base_version"].as_str() != Some(active.id.as_str()) {
        return json!({"ok": false, "retryable": true, "error": "Read the current note with read_minutes and pass its version_id as base_version."});
    }
    let Some(path) = active.content_path.clone() else {
        return json!({ "ok": false, "error": "노트 본문 파일 경로가 없음" });
    };
    let content = match tokio::fs::read_to_string(crate::storage::resolve(&path)).await {
        Ok(c) => c,
        Err(e) => return json!({ "ok": false, "error": format!("현재 노트 본문 로드 실패: {e}") }),
    };

    let (new_content, diffs, errors) = edit::apply_str_edits(&content, &edits);
    if !errors.is_empty() {
        tracing::debug!(errors = ?errors, "[edit] GUARD match-fail");
        return json!({ "ok": false, "error": errors.join("; "), "retryable": true });
    }
    if diffs.is_empty() {
        tracing::debug!("[edit] GUARD no-diff (적용된 편집 없음)");
        return json!({ "ok": false, "error": "적용된 편집이 없음", "retryable": true });
    }
    // 가드: 소스가 실제로 바뀌어야 통과. 변경이 *HTML 주석 추가뿐*이면(화면에 안 보임)
    // 거부. 공백·빈 줄은 마크다운에서 의미있는 변경이므로 보존해 비교(strip_comments).
    let src_changed = content != new_content;
    let comment_only =
        !src_changed || edit::strip_comments(&content) == edit::strip_comments(&new_content);
    tracing::debug!(
        diffs = diffs.len(),
        src_changed,
        comment_only,
        "[edit] guards"
    );
    if !src_changed {
        return json!({ "ok": false, "retryable": true,
            "error": "편집이 본문을 전혀 바꾸지 않았습니다. 실제로 바뀌도록 다시 시도하세요." });
    }
    if comment_only {
        return json!({ "ok": false, "retryable": true,
            "error": "변경이 주석(화면에 안 보이는 마크업)뿐입니다. 화면에 보이는 요소로 다시 시도하세요." });
    }

    // 새 버전 동기 생성 — 아카이브 + 완료본 + 제목 재파생 (Meetzy _create_minutes_version).
    let note = match notes::get(pool, note_id).await {
        Ok(n) => n,
        Err(e) => return json!({ "ok": false, "error": e.to_string() }),
    };
    let new_id = Uuid::new_v4().to_string();
    let path_str =
        crate::storage::body_rel(note_id, &new_id, crate::storage::body_ext_for(&new_content));
    let abs = crate::storage::resolve(&path_str);
    if let Some(parent) = abs.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return json!({ "ok": false, "error": format!("본문 저장 실패: {e}") });
        }
    }
    if let Err(e) = tokio::fs::write(&abs, new_content.as_bytes()).await {
        return json!({ "ok": false, "error": format!("본문 저장 실패: {e}") });
    }
    let refine_request = if user_request.is_empty() {
        format!("국소 편집 {}건", diffs.len())
    } else {
        user_request
    };
    let initial_content = active
        .initial_content_path
        .clone()
        .or_else(|| active.content_path.clone());
    let initial_ctx = active
        .initial_context_snapshot
        .clone()
        .or_else(|| Some(active.context_snapshot.clone()));
    if let Err(e) = note_bodies::archive_and_create_completed(
        pool,
        &new_id,
        note_id,
        active.transcript_id.as_deref(),
        &path_str,
        &generate::context_snapshot_json(&note),
        initial_content.as_deref(),
        initial_ctx.as_deref(),
        false,
        Some(&refine_request),
        Some(&active.id),
    )
    .await
    {
        let _ = tokio::fs::remove_file(&abs).await;
        return json!({ "ok": false, "retryable": true, "error": e.to_string() });
    }
    // 제목 = 본문 `# ` 헤딩 재파생 ('제목 바꿔줘' = 헤딩 편집).
    let _ = notes::update(
        pool,
        note_id,
        notes::UpdateNoteInput {
            title: Some(refine::extract_title(&new_content)),
            ..Default::default()
        },
    )
    .await;
    let _ = app.emit("note:updated", note_id.to_string());
    json!({ "ok": true, "minutes_id": new_id, "diffs": diffs })
}

/// echo 고유 — 테마 프리셋 전환. 본문은 그대로, notes.theme 만 바꾼다.
async fn set_theme(app: &AppHandle, pool: &DbPool, note_id: &str, args: &Value) -> Value {
    let theme = args
        .get("theme")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if theme.is_empty() {
        return json!({ "ok": false, "error": "theme(프리셋 id)가 비어 있습니다.", "retryable": true });
    }
    match notes::update(
        pool,
        note_id,
        notes::UpdateNoteInput {
            theme: Some(theme.clone()),
            ..Default::default()
        },
    )
    .await
    {
        Ok(_) => {
            let _ = app.emit("note:updated", note_id.to_string());
            json!({ "ok": true, "theme": theme })
        }
        Err(e) => json!({ "ok": false, "error": e.to_string(), "retryable": true }),
    }
}

/// write_note — 노트 필기형 본문 작성/수정 (refine::run_write, echo 고유).
async fn write_note_handler(app: &AppHandle, pool: &DbPool, note_id: &str, args: &Value) -> Value {
    let content = args["content"].as_str().unwrap_or("");
    let after = args["after"].as_str();
    let version = args["base_version"].as_str();
    match refine::run_insert(pool, note_id, content, after, version).await {
        Ok(body_id) => {
            let _ = app.emit("note:updated", note_id);
            json!({"ok": true, "note_body_id": body_id, "inserted_content": content, "status": "completed"})
        }
        Err(e) => json!({"ok": false, "retryable": true, "error": e.to_string()}),
    }
}

/// 로컬 앱 — 다운로드 URL 대신 정리된 녹음의 로컬 파일 경로를 반환한다. 프론트가
/// 파일 버튼(열기)으로 렌더하고, 모델에는 버튼이 표시됐다는 ack 만 간다.
async fn recording_file(pool: &DbPool, note_id: &str) -> Value {
    match recordings::list_for_note(pool, note_id).await {
        Ok(recs) => match recs.into_iter().find(|r| r.format == "webm") {
            Some(r) => json!({
                "ok": true,
                "file_path": crate::storage::resolve(&r.file_path).to_string_lossy().to_string(),
                "filename": r.original_filename,
            }),
            None => json!({ "ok": false, "error": "전달할 녹음 파일이 없습니다." }),
        },
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    }
}

/// 전사록 미리보기(1K)만 반환 — 전문은 프론트 TranscriptBlock 이 transcript_id 로
/// 온디맨드 조회. 모델 컨텍스트 폭증/원문 변형 방지(모델엔 ack 만 감).
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
    match tokio::fs::read_to_string(crate::storage::resolve(&path)).await {
        Ok(text) => {
            let total_chars = text.chars().count();
            let preview: String = text.chars().take(TRANSCRIPT_PREVIEW_CHARS).collect();
            let preview_chars = preview.chars().count();
            json!({
                "ok": true,
                "transcript_id": t.id,
                "preview": preview,
                "preview_chars": preview_chars,
                "total_chars": total_chars,
            })
        }
        Err(e) => json!({ "ok": false, "error": format!("전사록을 읽을 수 없습니다: {e}") }),
    }
}

/// Re-run transcription from scratch — ask_user 확인을 받은 다음 턴에만 LLM 이
/// 호출한다(도구 description 계약). Cleans prior transcripts + bodies.
async fn retry_transcribe(app: &AppHandle, pool: &DbPool, note_id: &str) -> Value {
    let recs = match recordings::list_for_note(pool, note_id).await {
        Ok(r) => r,
        Err(e) => return json!({ "ok": false, "error": e.to_string() }),
    };
    let Some(rec) = recs.into_iter().find(|r| r.format == "webm") else {
        return json!({ "ok": false, "error": "정리된 녹음이 없어 전사를 재시도할 수 없습니다." });
    };
    let cache = crate::storage::resolve(&format!(
        "{}/transcripts/{}.asr-cache",
        crate::storage::note_rel_dir(note_id),
        rec.id
    ));
    if cache.exists() {
        if let Err(e) = tokio::fs::remove_dir_all(cache).await {
            return json!({"ok": false, "error": e.to_string()});
        }
    }
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

    if let Some(t) = ts
        .iter()
        .rev()
        .find(|t| t.status == "failed" || t.status == "cancelled")
    {
        return match transcribe::dispatch(app, pool, note_id, t.recording_id.as_deref()).await {
            Ok(()) => {
                json!({"ok": true, "retried": "transcript", "eta_minutes": "1-10", "resumed": true})
            }
            Err(e) => json!({"ok": false, "error": e.to_string()}),
        };
    }

    json!({ "ok": false, "error": "재시작할 실패 작업이 없습니다. 현재 진행 중인 작업이 끝날 때까지 기다려주세요." })
}
