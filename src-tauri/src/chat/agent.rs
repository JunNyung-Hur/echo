//! Agent loop (Phase 3) — ports chat_agent/agent.py run_agent, non-streaming.
//!
//! One user message → up to 4 turns of (chat_with_tools → execute tools →
//! feed results back). Emits `chat:status` / `chat:done` Tauri events for the
//! UI; persists each turn to note_chat_messages (G-SSE-001 turn merge).
//! read_transcript splices content + short-circuits (G-CHAT-014).

#![allow(dead_code)] // Phase 3: invoked by the chat_send command.

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use std::time::Duration;

use uuid::Uuid;

use crate::ai;
use crate::chat::{exec, intent, prompt, refine, tools};
use crate::db::DbPool;
use crate::error::{Error, Result};
use crate::models::ChatMessage;
use crate::repo::{ai_endpoints, chat as chat_repo, note_bodies, notes, recordings, transcripts};
use crate::worker::transcribe;

const MAX_TURNS: usize = 4;

/// Run one agent turn for `user_message`. Persists the user message + any
/// assistant turns, emits status events. Errors surface as an assistant chat
/// message (so the user always sees something) rather than bubbling up.
pub async fn run_agent(
    app: &AppHandle,
    pool: &DbPool,
    note_id: &str,
    user_message: &str,
    user_state: Option<Value>,
) -> Result<()> {
    // History as it stood before this user message.
    let history = chat_repo::list_for_note(pool, note_id)
        .await
        .unwrap_or_default();

    // 첨부 녹음(freeform 전송)이면 전사→노트 반영 경로로 분기한다.
    let recording_ids = extract_recording_ids(user_state.as_ref());
    let note_type = notes::get(pool, note_id).await.ok().and_then(|n| n.note_type);
    let is_attach = note_type.as_deref() == Some("freeform") && !recording_ids.is_empty();

    // 유저 메시지 저장(텍스트 그대로 — 빈 첨부는 버블의 칩으로 표현됨). 첨부 녹음은
    // 이 메시지에 연결하고 consumed 처리한다(버블 칩 + 보관함 이동).
    let user_msg_id = chat_repo::create(pool, note_id, "user", user_message, None, None).await?;
    if !recording_ids.is_empty() {
        if let Err(e) = recordings::link_to_message(pool, &recording_ids, &user_msg_id).await {
            tracing::warn!(?e, %note_id, "failed to link attached recordings to message");
        }
    }
    let _ = app.emit(
        "chat:status",
        json!({ "note_id": note_id, "state": "thinking" }),
    );

    let result = if is_attach {
        run_attachment_turn(app, pool, note_id, user_message, &recording_ids).await
    } else if note_type.as_deref() == Some("freeform") {
        // freeform 텍스트 턴은 의도 스테이지로 라우팅(인사·잡담·질문 vs 노트 콘텐츠).
        run_freeform_text(app, pool, note_id, user_message, &history).await
    } else {
        run_inner(app, pool, note_id, user_message, user_state, &history).await
    };
    if let Err(e) = result {
        tracing::warn!(?e, %note_id, "agent run failed");
        let _ = chat_repo::create(
            pool,
            note_id,
            "assistant",
            &format!("문제가 생겼어요: {e}"),
            None,
            None,
        )
        .await;
    }
    let _ = app.emit("chat:done", json!({ "note_id": note_id }));
    Ok(())
}

/// Pull `recordingIds` (freeform 첨부) out of the passthrough user_state.
fn extract_recording_ids(user_state: Option<&Value>) -> Vec<String> {
    user_state
        .and_then(|s| s.get("recordingIds"))
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

/// freeform 첨부 전송 처리: 각 녹음을 전사한 뒤(진행 status emit), map-reduce로 기존
/// 노트와 통합 반영하고 결과 메시지를 남긴다. 기존 노트도 통합 입력 중 하나로 다뤄
/// 본문 손실을 막고, 주제가 다른 여러 녹음에도 대응한다.
async fn run_attachment_turn(
    app: &AppHandle,
    pool: &DbPool,
    note_id: &str,
    user_message: &str,
    recording_ids: &[String],
) -> Result<()> {
    let total = recording_ids.len();
    let mut transcripts: Vec<String> = Vec::new();
    for (i, rid) in recording_ids.iter().enumerate() {
        let _ = app.emit(
            "chat:status",
            json!({ "note_id": note_id, "state": "transcribing", "current": i + 1, "total": total }),
        );
        match transcribe_and_wait(app, pool, note_id, rid).await {
            Ok(Some(text)) if !text.trim().is_empty() => transcripts.push(text),
            Ok(_) => {}
            Err(e) => tracing::warn!(?e, %rid, "attachment transcribe failed"),
        }
    }

    if transcripts.is_empty() {
        chat_repo::create(
            pool,
            note_id,
            "assistant",
            "녹음에서 옮길 내용을 찾지 못했어요. 다시 시도해 주세요.",
            None,
            None,
        )
        .await?;
        return Ok(());
    }

    // map(각 전사 초안) → reduce(기존 노트 + 초안 통합). status는 내부에서 emit.
    let body_id = refine::run_map_reduce(app, pool, note_id, user_message, &transcripts).await?;

    let msg = if total == 1 {
        "녹음을 노트에 옮겼어요.".to_string()
    } else {
        format!("녹음 {total}개를 노트에 옮겼어요.")
    };
    chat_repo::create(pool, note_id, "assistant", &msg, None, Some(&body_id)).await?;
    let _ = app.emit("note:updated", note_id.to_string());
    Ok(())
}

/// Transcribe one recording and block until it finishes, returning the text.
/// Reuses the spawned transcribe task (timeout/cancellation handled there) and
/// polls the row to terminal status. None on empty/failed/cancelled.
async fn transcribe_and_wait(
    app: &AppHandle,
    pool: &DbPool,
    note_id: &str,
    recording_id: &str,
) -> Result<Option<String>> {
    let task_id = Uuid::new_v4().to_string();
    let t = transcripts::create_processing(pool, note_id, Some(recording_id), &task_id).await?;
    transcribe::spawn(app.clone(), t.id.clone(), task_id);
    loop {
        tokio::time::sleep(Duration::from_millis(800)).await;
        let cur = transcripts::get(pool, &t.id).await?;
        match cur.status.as_str() {
            "completed" => {
                let path = cur.corrected_path.or(cur.raw_path);
                return Ok(match path {
                    Some(p) => tokio::fs::read_to_string(crate::storage::resolve(&p)).await.ok(),
                    None => None,
                });
            }
            "failed" | "cancelled" | "empty" => return Ok(None),
            _ => {}
        }
    }
}

// 565309d — 응답 언어 결정. ui_lang(설정)을 anchor 로, 메시지가 *명백히* 반대
// 언어일 때만 전환한다(짧은 ack/숫자/파일명 등으로 인한 역슬립 방지).
fn decide_response_lang(ui_lang: Option<&str>, msg: &str) -> &'static str {
    let ui_en = ui_lang == Some("en");
    let has_hangul = msg.chars().any(|c| ('가'..='힣').contains(&c));
    if ui_en {
        // 영어 모드: 한국어 음절은 명백한 전환 신호.
        if has_hangul {
            "ko"
        } else {
            "en"
        }
    } else {
        // 한국어 모드: 한글 있으면 ko. 없어도 ko 유지하되, 영단어(2자+) 3개 이상의
        // 실질적 영어 문장일 때만 en.
        if !has_hangul && count_en_words(msg) >= 3 {
            "en"
        } else {
            "ko"
        }
    }
}

// `[A-Za-z]{2,}` 매치 개수 (정규식 없이 — 2자 이상 영문 런을 센다).
fn count_en_words(s: &str) -> usize {
    let mut count = 0usize;
    let mut run = 0usize;
    for c in s.chars() {
        if c.is_ascii_alphabetic() {
            run += 1;
        } else {
            if run >= 2 {
                count += 1;
            }
            run = 0;
        }
    }
    if run >= 2 {
        count += 1;
    }
    count
}

/// freeform 텍스트 턴: 의도 분류(intent stage) → content/edit이면 refine::run_write로
/// 노트에 반영, social/smalltalk/question이면 대화로만 응대한다. 무거운 본문 쓰기는
/// 종전과 동일하게 run_write가 담당하고, 여기서는 "무엇을 원하는가"만 가른다.
async fn run_freeform_text(
    app: &AppHandle,
    pool: &DbPool,
    note_id: &str,
    user_message: &str,
    history: &[ChatMessage],
) -> Result<()> {
    let llm = ai_endpoints::get_active(pool, "llm")
        .await?
        .ok_or_else(|| Error::Other("활성 LLM endpoint가 없습니다. 설정에서 등록하세요.".into()))?;

    // 현재 노트 본문(스타일 제외) — question 응답 + edit 판단 근거.
    let active = note_bodies::get_active(pool, note_id).await?;
    let body_text: Option<String> = match &active {
        Some(b) => match &b.content_path {
            Some(p) => tokio::fs::read_to_string(crate::storage::resolve(p))
                .await
                .ok()
                .map(|h| refine::split_body_style(&h).0),
            None => None,
        },
        None => None,
    };

    let ui_lang = crate::repo::settings::get(pool, "ui_lang").await.ok().flatten();
    let response_lang = decide_response_lang(ui_lang.as_deref(), user_message);

    let decision =
        intent::classify(&llm, body_text.as_deref(), history, user_message, response_lang).await?;

    let mut body_version: Option<String> = None;
    if decision.writes() {
        match refine::run_write(app, pool, note_id, user_message, decision.write_intent()).await {
            Ok(bid) => body_version = Some(bid),
            Err(e) => {
                tracing::warn!(?e, %note_id, "freeform write failed");
                let msg = format!("노트에 반영하지 못했어요: {e}");
                let _ = app.emit("chat:delta", json!({ "note_id": note_id, "delta": msg }));
                chat_repo::create(pool, note_id, "assistant", &msg, None, None).await?;
                return Ok(());
            }
        }
    }

    let reply = if !decision.reply.trim().is_empty() {
        decision.reply.clone()
    } else if response_lang == "en" {
        if body_version.is_some() { "Got it down." } else { "Go ahead." }.to_string()
    } else if body_version.is_some() {
        "적어뒀어요.".to_string()
    } else {
        "네, 말씀하세요.".to_string()
    };

    let _ = app.emit("chat:delta", json!({ "note_id": note_id, "delta": reply }));
    chat_repo::create(pool, note_id, "assistant", &reply, None, body_version.as_deref()).await?;
    Ok(())
}

async fn run_inner(
    app: &AppHandle,
    pool: &DbPool,
    note_id: &str,
    user_message: &str,
    user_state: Option<Value>,
    history: &[ChatMessage],
) -> Result<()> {
    let note = notes::get(pool, note_id).await?;
    let recordings = recordings::list_for_note(pool, note_id).await?;
    let transcripts = transcripts::list_for_note(pool, note_id).await?;
    let bodies = note_bodies::list_for_note(pool, note_id).await?;
    let llm = ai_endpoints::get_active(pool, "llm")
        .await?
        .ok_or_else(|| Error::Other("활성 LLM endpoint가 없습니다. 설정에서 등록하세요.".into()))?;

    // Active completed body, style-stripped, for content Q&A in the prompt.
    let active_body: Option<String> = match bodies
        .iter()
        .find(|b| b.archived == 0 && b.status == "completed")
    {
        Some(b) => match &b.content_path {
            Some(p) => tokio::fs::read_to_string(crate::storage::resolve(p))
                .await
                .ok()
                .map(|h| refine::split_body_style(&h).0),
            None => None,
        },
        None => None,
    };

    // Worker timeline (newest first).
    let timeline: Vec<(String, String, Option<String>)> =
        sqlx::query_as::<_, (String, String, String)>(
            "SELECT kind, content, created_at FROM note_timeline_events WHERE note_id = ? ORDER BY created_at DESC LIMIT 20",
        )
        .bind(note_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(k, c, t)| (k, c, Some(t)))
        .collect();

    // 노트 필기형 → stage="freeform"으로 매핑(write_note 노출). 그 외엔 녹음/전사/본문 기반.
    let stage = if note.note_type.as_deref() == Some("freeform") {
        "freeform"
    } else {
        prompt::derive_stage(&recordings, &transcripts, &bodies)
    };

    // Capability gating — strip tools whose UI action is `hidden`.
    let hidden: Vec<String> = user_state
        .as_ref()
        .and_then(|s| s.get("available_actions"))
        .and_then(|v| v.as_object())
        .map(|actions| {
            actions
                .values()
                .filter_map(|a| {
                    if a.get("state").and_then(|v| v.as_str()) == Some("hidden") {
                        a.get("ai_tool").and_then(|v| v.as_str()).map(String::from)
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let tool_specs = tools::tools_for(stage, &hidden);

    // 565309d — 출력 언어: ui_lang(설정) anchor + 발화 감지. 서버에서 결정해 주입.
    let ui_lang = crate::repo::settings::get(pool, "ui_lang").await.ok().flatten();
    let response_lang = decide_response_lang(ui_lang.as_deref(), user_message);

    let ctx = prompt::PromptCtx {
        note_title: &note.title,
        note_started_at: &note.started_at,
        note_location: &note.location,
        note_language: &note.language,
        recordings: &recordings,
        transcripts: &transcripts,
        bodies: &bodies,
        active_body: active_body.as_deref(),
        timeline: &timeline,
        user_state: user_state.as_ref(),
        response_lang,
        note_type: note.note_type.as_deref(),
    };
    let system = prompt::build_system_prompt(&ctx);

    let mut messages: Vec<Value> = vec![json!({ "role": "system", "content": system })];
    messages.extend(serialize_history(history, &timeline));
    messages.push(json!({ "role": "user", "content": user_message }));

    // Carried across turns: the body version a refine produced this send, so the
    // "이 시점 본문 보기" chip lands on the *visible* final assistant message.
    // The tool-call turn itself usually has empty content (the model emits only
    // the call), so it's filtered out of the feed — attaching the chip there
    // would make it vanish. Ports the old turn-merge pending_minutes_version.
    let mut body_version: Option<String> = None;

    for _ in 0..MAX_TURNS {
        let turn = ai::chat_with_tools_streaming(&llm, &messages, &tool_specs, |d| {
            let _ = app.emit("chat:delta", json!({ "note_id": note_id, "delta": d }));
        })
        .await?;

        if turn.tool_calls.is_empty() {
            chat_repo::create(
                pool,
                note_id,
                "assistant",
                &turn.content,
                None,
                body_version.as_deref(),
            )
            .await?;
            return Ok(());
        }

        // Assistant message carrying the tool_calls (for the next turn's context).
        let api_calls: Vec<Value> = turn
            .tool_calls
            .iter()
            .map(|tc| {
                json!({
                    "id": tc.id,
                    "type": "function",
                    "function": { "name": tc.name, "arguments": tc.args.to_string() }
                })
            })
            .collect();
        messages.push(json!({
            "role": "assistant",
            "content": if turn.content.is_empty() { Value::Null } else { json!(turn.content) },
            "tool_calls": api_calls,
        }));

        let mut persisted: Vec<Value> = Vec::new();
        let mut extra = String::new();
        let mut short_circuit = false;

        for tc in &turn.tool_calls {
            let _ = app.emit(
                "chat:status",
                json!({ "note_id": note_id, "state": "tool", "tool": tc.name }),
            );
            let result = exec::execute_tool(app, pool, note_id, &tc.name, &tc.args).await;
            let ok = result.get("ok").and_then(|v| v.as_bool()) == Some(true);

            if tc.name == "refine_minutes" && ok {
                body_version = result
                    .get("minutes_id")
                    .and_then(|v| v.as_str())
                    .map(String::from);
            }
            // G-CHAT-014 — splice transcript content + short-circuit the loop.
            if tc.name == "read_transcript" && ok {
                if let Some(c) = result.get("content").and_then(|v| v.as_str()) {
                    if !turn.content.is_empty() && !turn.content.ends_with('\n') {
                        extra.push_str("\n\n");
                    }
                    extra.push_str(c);
                    // 1f207ab — no "잘렸음" notice; the 전체보기 affordance lives
                    // on the frontend TranscriptBlock.
                    short_circuit = true;
                }
            }

            messages.push(
                json!({ "role": "tool", "tool_call_id": tc.id, "content": result.to_string() }),
            );
            persisted
                .push(json!({ "id": tc.id, "name": tc.name, "args": tc.args, "result": result }));
        }

        let content = format!("{}{}", turn.content, extra);
        // Defer the chip to the visible final message — unless we're about to
        // return now (short-circuit), in which case attach it here so it isn't
        // lost (this turn's content carries the transcript, so it's visible).
        let chip = if short_circuit {
            body_version.as_deref()
        } else {
            None
        };
        chat_repo::create(
            pool,
            note_id,
            "assistant",
            &content,
            Some(&Value::Array(persisted).to_string()),
            chip,
        )
        .await?;

        if short_circuit {
            return Ok(());
        }
    }
    Ok(())
}

/// Persisted chat history → OpenAI messages, expanding assistant turns that
/// carried tool_calls into (assistant + tool_result) pairs. (ports _serialize_history)
fn serialize_history(
    history: &[ChatMessage],
    timeline: &[(String, String, Option<String>)],
) -> Vec<Value> {
    // e18f27d/91890a7 — merge worker timeline events into the chat history in
    // chronological order as `user`-role "[진행 상황] …" messages, so a weak
    // model notices a later "전사 완료" instead of paraphrasing a stale "전사 중"
    // reply. (user, NOT system: many chat models reject a system message that
    // isn't at the very beginning.) Tie on created_at → chat(0) before
    // timeline(1) so a user turn precedes the event it triggered.
    enum Item<'a> {
        Chat(&'a ChatMessage),
        Timeline(&'a str),
    }
    let mut merged: Vec<(&str, i32, usize, Item)> = Vec::new();
    for (i, m) in history.iter().enumerate() {
        merged.push((m.created_at.as_str(), 0, i, Item::Chat(m)));
    }
    for (i, (_, content, created)) in timeline.iter().enumerate() {
        merged.push((created.as_deref().unwrap_or(""), 1, i, Item::Timeline(content)));
    }
    merged.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)));

    let mut out: Vec<Value> = Vec::new();
    for (_, _, _, item) in merged {
        let m = match item {
            Item::Timeline(content) => {
                out.push(json!({ "role": "user", "content": format!("[진행 상황] {content}") }));
                continue;
            }
            Item::Chat(m) => m,
        };
        if m.role == "user" {
            out.push(json!({ "role": "user", "content": m.content }));
            continue;
        }
        let tcs: Vec<Value> = m
            .tool_calls
            .as_deref()
            .and_then(|s| serde_json::from_str::<Vec<Value>>(s).ok())
            .unwrap_or_default();
        if tcs.is_empty() {
            out.push(json!({ "role": "assistant", "content": m.content }));
            continue;
        }
        let api_calls: Vec<Value> = tcs
            .iter()
            .enumerate()
            .map(|(i, tc)| {
                let id = tc["id"]
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| format!("call_{}_{}", m.id, i));
                json!({
                    "id": id,
                    "type": "function",
                    "function": { "name": tc["name"], "arguments": tc["args"].to_string() }
                })
            })
            .collect();
        out.push(json!({
            "role": "assistant",
            "content": if m.content.is_empty() { Value::Null } else { json!(m.content) },
            "tool_calls": api_calls,
        }));
        for (i, tc) in tcs.iter().enumerate() {
            let id = tc["id"]
                .as_str()
                .map(String::from)
                .unwrap_or_else(|| format!("call_{}_{}", m.id, i));
            out.push(
                json!({ "role": "tool", "tool_call_id": id, "content": tc["result"].to_string() }),
            );
        }
    }
    out
}
