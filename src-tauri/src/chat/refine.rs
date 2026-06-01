//! Minutes refine pipeline (Phase 3) — Stage 2. Ports generate.py refine_minutes
//! + the body/style channel split (G-REFINE-001/002) and archive-and-create
//! versioning (G-REFINE-003/004, G-VERSION-002/003/004).

#![allow(dead_code)] // Phase 3: invoked by the refine_minutes tool handler.

use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::ai;
use crate::db::DbPool;
use crate::error::{Error, Result};
use crate::models::{Note, NoteBody};
use crate::prompts;
use crate::repo::{ai_endpoints, note_bodies, notes, transcripts};

/// G-REFINE-001 — split the inline `<style>` block off the body so the BODY
/// channel the refine LLM sees carries no CSS signal. Returns
/// `(body_without_style, first_style_block)`.
pub fn split_body_style(html: &str) -> (String, String) {
    let lower = html.to_lowercase();
    if let Some(start) = lower.find("<style") {
        if let Some(end_rel) = lower[start..].find("</style>") {
            let end = start + end_rel + "</style>".len();
            let style = html[start..end].to_string();
            let mut body = String::with_capacity(html.len());
            body.push_str(&html[..start]);
            body.push_str(&html[end..]);
            return (body, style);
        }
    }
    (html.to_string(), String::new())
}

/// G-REFINE-002 — if the LLM dropped the `<style>` block, splice the fallback
/// back into `<head>` so the rendered document isn't unstyled. No-op if the
/// output already has style or there's no `<head>`.
pub fn ensure_style_block(html: &str, fallback_style: &str) -> String {
    if fallback_style.is_empty() || html.to_lowercase().contains("<style") {
        return html.to_string();
    }
    let lower = html.to_lowercase();
    if let Some(idx) = lower.find("</head>") {
        let mut out = String::with_capacity(html.len() + fallback_style.len());
        out.push_str(&html[..idx]);
        out.push_str(fallback_style);
        out.push_str(&html[idx..]);
        return out;
    }
    html.to_string()
}

/// Ports generate.py `_build_refine_user_content`. The two leading sections —
/// current meeting context + the just-changed fields — are what let the LLM
/// reflect a metadata edit (e.g. a new title) into the body's <h1>; without
/// them the refine model never sees the new value and the change can't land.
fn build_refine_user_content(
    meeting_info: &str,
    updated: &[(String, String)],
    transcript: Option<&str>,
    body: &str,
    style: &str,
    user_request: &str,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !meeting_info.trim().is_empty() {
        parts.push(format!("[Meeting context (current)]\n{}", meeting_info.trim()));
    }
    if !updated.is_empty() {
        let lines: Vec<String> = updated
            .iter()
            .map(|(label, val)| format!("- {}: {}", label, if val.is_empty() { "(empty)" } else { val }))
            .collect();
        parts.push(format!(
            "[Updated meeting context — these fields just changed]\n{}",
            lines.join("\n")
        ));
    }
    parts.push(format!(
        "[Latest user message — apply this request to the minutes]\n{}",
        user_request.trim()
    ));
    if let Some(t) = transcript {
        if !t.trim().is_empty() {
            parts.push(format!(
                "[Transcript (reference, read-only — do not output)]\n{}",
                t.trim()
            ));
        }
    }
    parts.push(format!("[Current minutes body]\n{}", body));
    parts.push(format!("[Current minutes style]\n{}", style));
    parts.join("\n\n")
}

/// Fields whose current note value differs from the body's generation-time
/// snapshot — surfaced to the refine LLM so "이 변경 반영해줘" knows the new
/// value (e.g. the title to write into the <h1>). Mirrors the frontend badge.
fn updated_context_fields(note: &Note, snapshot_json: &str) -> Vec<(String, String)> {
    let snap: serde_json::Value = serde_json::from_str(snapshot_json).unwrap_or(serde_json::Value::Null);
    let s = |k: &str| snap.get(k).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let mut out = Vec::new();
    if s("title") != note.title.trim() {
        out.push(("제목".to_string(), note.title.trim().to_string()));
    }
    let loc = note.location.as_deref().unwrap_or("").trim().to_string();
    if s("location") != loc {
        out.push(("장소".to_string(), loc));
    }
    if s("language") != note.language.trim() {
        out.push(("언어".to_string(), note.language.trim().to_string()));
    }
    let sa = note.started_at.as_deref().unwrap_or("").trim().to_string();
    if s("started_at") != sa {
        out.push(("시작 시각".to_string(), sa));
    }
    out
}

async fn read_transcript_text(pool: &DbPool, transcript_id: &str) -> Option<String> {
    let t = transcripts::get(pool, transcript_id).await.ok()?;
    let path = t.corrected_path.or(t.raw_path)?;
    tokio::fs::read_to_string(&path).await.ok()
}

/// Refine the active body per the user's natural-language request, producing a
/// new completed version. Returns the new body id. Synchronous (the chat tool
/// awaits it). On no active body / no LLM endpoint, returns an Err the agent
/// surfaces to the user.
pub async fn run_refine(
    app: &AppHandle,
    pool: &DbPool,
    note_id: &str,
    user_request: &str,
) -> Result<String> {
    let active = note_bodies::get_active(pool, note_id)
        .await?
        .ok_or_else(|| Error::Other("활성 노트가 없어 다듬을 수 없습니다.".into()))?;
    let content_path = active
        .content_path
        .clone()
        .ok_or_else(|| Error::Other("노트 본문 파일 경로가 없습니다.".into()))?;
    let current_html = tokio::fs::read_to_string(&content_path).await?;

    let llm = ai_endpoints::get_active(pool, "llm")
        .await?
        .ok_or_else(|| Error::Other("활성 LLM endpoint가 없습니다. 설정에서 등록하세요.".into()))?;

    // Current meeting meta + the fields that drifted from the body's snapshot,
    // so a "제목 반영" request actually has the new title to write in.
    let note = notes::get(pool, note_id).await?;
    let meeting_info = crate::worker::generate::build_meeting_info(&note);
    let updated = updated_context_fields(&note, &active.context_snapshot);
    let transcript = match &active.transcript_id {
        Some(tid) => read_transcript_text(pool, tid).await,
        None => None,
    };

    let (body, style) = split_body_style(&current_html);
    let user_content = build_refine_user_content(
        &meeting_info,
        &updated,
        transcript.as_deref(),
        &body,
        &style,
        user_request,
    );
    let result =
        ai::chat_completion(&llm, prompts::MINUTES_REFINE_SYSTEM_PROMPT, &user_content).await?;
    let mut html = result.content;
    if html.trim().is_empty() {
        return Err(Error::Other("LLM이 빈 본문을 반환했습니다.".into()));
    }
    // G-REFINE-002 — re-splice style if the model dropped it.
    html = ensure_style_block(&html, &style);

    // Persist the new version + archive the old one (G-REFINE-003).
    let new_id = Uuid::new_v4().to_string();
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| Error::Other(format!("app_data_dir: {e}")))?
        .join("note_bodies")
        .join(&new_id)
        .join("content.html");
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, html.as_bytes()).await?;
    let path_str = path.to_string_lossy().to_string();

    // G-VERSION-004 — carry the stage-1 baseline forward across versions.
    let initial_content = active
        .initial_content_path
        .clone()
        .or_else(|| active.content_path.clone());
    let initial_ctx = active
        .initial_context_snapshot
        .clone()
        .or_else(|| Some(active.context_snapshot.clone()));

    note_bodies::archive_and_create_completed(
        pool,
        &new_id,
        note_id,
        active.transcript_id.as_deref(),
        &path_str,
        // G-VERSION — the new version's snapshot is the *current* note meta, so
        // once a metadata change is refined into the body the "meta drifted"
        // badge clears (snapshot.title now matches note.title). Ports tools.py.
        &crate::worker::generate::context_snapshot_json(&note),
        initial_content.as_deref(),
        initial_ctx.as_deref(),
        false,
    )
    .await?;

    // 제목=본문 첫 줄 — refine로 본문(특히 첫 줄)이 바뀌면 notes.title도 동기화한다.
    // (generate/run_write와 동일 규칙; 안 하면 "제목 바꿔줘"가 메타에 반영 안 됨.)
    let _ = notes::update(
        pool,
        note_id,
        notes::UpdateNoteInput {
            title: Some(extract_title(&html)),
            ..Default::default()
        },
    )
    .await;

    Ok(new_id)
}

/// 노트 필기형(freeform) 본문 작성. 사람이 노트 필기하듯, 채팅/녹음 내용을 노트에
/// 받아적는다. 기존 본문이 있으면 그 위에 **이어쓰기/통합**(전체 재생성 X), 없으면
/// 새로 시작. 디자인 기본은 노란 노트패드(사용자가 "바꿔줘"하면 변경).
const FREEFORM_WRITE_SYSTEM_PROMPT: &str = "당신은 사용자의 노트를 대신 받아적고 정리하는 필기 도우미입니다.\n- 사용자가 채팅(또는 녹음 전사)으로 전달한 내용을 노트 본문(HTML)에 반영합니다.\n- **두 가지 작업 모드를 구분하세요.** (가) **받아적기** — 사용자가 새 내용을 말하면 기존 본문은 전부 그대로 두고 새 내용만 알맞은 위치(관련 섹션 끝 등)에 한 줄씩 이어 적습니다. 기존 항목을 누락하거나 덮어쓰지 마세요. (나) **정리/재구성** — '정리해줘/묶어줘/표로/타임라인으로/요약/~형식으로'처럼 기존 내용을 손보라는 요청이면, 그 대상 범위를 **처음부터 다시 써서 중복 없는 완성본으로 교체**합니다. 정리본을 기존 줄 아래에 새로 덧붙이지 마세요 — 같은 내용이 두 번 남으면 실패입니다. 본문이 비어 있으면 새로 시작합니다.\n- 사용자가 적어달라는 내용을 노트에 담되, **사람이 필기하듯 구어체 종결어미를 떼고 개조식 명사형으로 정돈해 적으세요: '있고'→'있음', '핵심이야'→'핵심', '안 된대'→'안 됨', '~래/~대'(전언)→'~임', '~했어'→'~함', '~하자/~하기로'(제안·결정 메모)→'~' 또는 '~하기'. 어미만 다듬고 단어·수치·고유명사·질문형('왜 비쌀까?')은 의미 그대로 보존하며 정보를 잃지 마세요.** 너무 장황하면 핵심 위주로 깔끔히. 단편적인 한 줄도 노트의 한 항목(예: 글머리)으로 적습니다.\n- **본문을 손보는 작업의 범위는 user_content 맨 위의 [작업 모드] 지시를 최우선으로 따릅니다 — 받아적기는 이어쓰기만, 문체 정돈은 구어체 어미만 다듬고 구조·제목은 그대로 두며, 구조 재구성일 때만 관련 항목을 소제목(`<h2>`)·글머리(`<ul><li>`)로 묶고 맨 위 제목 줄을 `<h1>`으로 올립니다.** 작업 모드가 문체 정돈이면 절대 새 소제목·불릿 구조를 만들지 마세요. 어느 모드든 같은 내용을 다루는 줄을 중복으로 남기지 마세요 — 정돈·재배치 시 원본 줄을 그대로 둔 채 정리본을 따로 추가하면 안 되고, 원본 줄 자체를 바꿉니다('환불 요청 들어옴'→'현황: 환불 요청'). '표로/타임라인으로' 재구성이면 (1) 정리 결과를 만들고 (2) 거기 반영된 기존 평문 줄은 삭제하며 (3) 표 칸에 안 들어가는 정보(제목·금액·상태·메모 등)만 평문으로 보존합니다. 단 '강조해줘·표로 바꿔줘·정리해줘' 같은 편집·서식 지시 문장 자체는 노트 정보가 아니므로 본문에 적거나 남기지 마세요.\n- **'정리'는 작업의 끝이 아니라 중간 정돈입니다 — 정리(정돈·표·타임라인) 후에도 새 입력이 오면, 새로 빈 노트를 시작하지 말고 정돈된 구조(섹션·표 등)에 자연스럽게 이어서 누적하세요.**\n- **부분편집·구조 조작 명령에는 지정된 부분만 정확히 조작하고 나머지 본문은 전부 보존하세요: 'A부터 B까지는 ~로 묶어줘'=해당 항목들을 한 섹션/그룹으로 묶기, 'X는 Y로 빼자/바꿔'=그 값·항목만 변경, '이 섹션 제목은 ~로 하자'=그 섹션 제목만 지정·변경, 'X는 (저 섹션에서) 제거/빼줘'=그 항목만 삭제하고 나머지는 그대로 유지. 단 이 부분편집 지시 문장 자체('~로 바꿔줘', '~로 묶어줘', '~ 제거해줘', '섹션 제목을 ~로')는 노트 내용이 아니므로 본문에 절대 적지 마세요 — 명령은 실행만 하고, 조작 대상이 본문에 없거나 모호하면 본문을 그대로 두되 지시문을 새로 적지 않습니다.**\n- **정정 입력('아 X 아니라 Y', 'X가 맞다', 'Y였다', '아 근데 X 아니라 Y였네')은 기존 본문에서 옛 값(X)을 찾아 새 값(Y)으로 반드시 교체**하세요. 이름·직급·호칭·숫자뿐 아니라 항목의 내용·서술(예: '점심 제공'→'식대 지원')도 무엇을 정정하든 옛 값을 본문에 그대로 남기지 말고 새 값으로 바꿔야 합니다. (예: 본문에 '박부장'이 있고 '박부장 아니라 박차장'이라고 하면 '박부장'을 '박차장'으로 교체.) **특히 '복지로 점심 제공'처럼 앞부분('복지로')이 같고 뒷부분만 바뀌는 정정이면, 새 줄을 따로 추가하지 말고 그 기존 줄의 뒷부분만 고쳐 옛 값('점심 제공')이 본문에 절대 남지 않게 하세요 — 정정은 줄 추가가 아니라 기존 줄 수정입니다.**\n- **스타일(색·폰트·줄·배경·패딩)을 일절 넣지 마세요. 노트 템플릿이 디자인을 담당합니다.** `<html>`/`<head>`/`<style>`/`<body>` 태그도 없이 **본문 콘텐츠 조각**만 생성합니다 — `<h2>`, `<p>`, `<ul><li>`, `<table>` 같은 시맨틱 태그와 글자만. style 속성 금지.\n- **당신(agent) 자신의 멘트는 본문에 절대 넣지 마세요.** '앞으로 ~하겠습니다', '반영했습니다', '적었습니다', '수정할까요' 같은 약속·확인·안내 문구는 노트 콘텐츠가 아닙니다 — 노트에 담을 실제 내용만 적습니다.\n- **사용자가 '질문 → 답' 형식이나 특정 표현·문구를 지정하면 그대로 반영**하세요. 질문을 빼고 답만 적지 마세요.\n- '~라는 질문이 누락됐어', '~가 빠졌어', '~ 틀렸어' 같은 **지적·피드백 문장 자체는 본문에 넣지 말고, 무엇을 고치라는 수정 지시로 해석**해 본문을 고치세요.\n- **노트의 표시 제목은 본문 맨 위 첫 줄에서 자동으로 추출됩니다. 사용자가 '제목을 ○○로 해줘'나 '제목 달아줘'라고 하면, 본문 맨 위 첫 줄을 그 제목으로 만드세요 — 기존 첫 줄이 제목이면 교체하고, 없으면 맨 위에 제목 한 줄을 추가합니다. 제목도 평문으로 적습니다.**\n- '앞으로 ~해줘', '항상 ~하게', '다음부터 ~' 같은 **미래 규칙·지시 문구는 지금 노트에 적을 내용이 아닙니다 — 본문에 절대 넣지 마세요.**\n- **'이제 ~ 보자', '~로 넘어가자', '다음은 ~', '~ 얘기인데/얘기로 가서' 같은 화제 전환·도입 발화 자체는 본문에 적지 마세요 — 새 주제로 넘어간다는 신호일 뿐입니다. 뒤따르는 실제 내용과 그 주제명만 (필요하면 새 섹션 제목으로) 적고, '이제 ~ 보자' 같은 도입 군더더기는 버리세요.**\n- 이 노트는 **줄 친 노란 공책**입니다. **각 항목·발화는 반드시 별도의 줄로 분리하세요 — 한 항목당 하나의 `<p>`(또는 `<ul><li>`) 블록으로 만들고, 서로 다른 항목 여러 개를 공백·쉼표로 한 줄이나 한 문단에 이어붙이면 절대 안 됩니다.** 긴 한 문단보다 짧은 항목·글머리 위주로. 설명 없이 콘텐츠 조각만, 언어는 사용자 발화 언어를 따릅니다.";

/// 본문 HTML의 첫 의미있는 텍스트 줄을 노트 제목으로 추출(태그 제거, 빈 본문은 "(제목 없음)").
pub(crate) fn extract_title(html: &str) -> String {
    // full HTML 문서(minutes)면 <body> 이후만 본다 — <head>/<style> 안의 CSS가
    // 첫 줄로 뽑혀 "body {" 같은 제목이 되는 걸 막는다. 콘텐츠 조각(freeform)은 그대로.
    let scope = match html.find("<body") {
        Some(idx) => {
            let after = &html[idx..];
            after.find('>').map(|g| &after[g + 1..]).unwrap_or(html)
        }
        None => html,
    };
    let mut text = String::new();
    let mut in_tag = false;
    for c in scope.chars() {
        match c {
            '<' => {
                in_tag = true;
                text.push('\n');
            }
            '>' => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    text.lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .map(|l| l.chars().take(120).collect::<String>())
        .unwrap_or_else(|| "제목 없음".to_string())
}

pub async fn run_write(
    app: &AppHandle,
    pool: &DbPool,
    note_id: &str,
    user_request: &str,
    intent: &str,
) -> Result<String> {
    let llm = ai_endpoints::get_active(pool, "llm")
        .await?
        .ok_or_else(|| Error::Other("활성 LLM endpoint가 없습니다. 설정에서 등록하세요.".into()))?;
    let note = notes::get(pool, note_id).await?;
    let active = note_bodies::get_active(pool, note_id).await?;

    // 기존 본문(있으면)을 읽어 이어쓰기 context로.
    let current_html = match &active {
        Some(b) => match &b.content_path {
            Some(p) => tokio::fs::read_to_string(p).await.unwrap_or_default(),
            None => String::new(),
        },
        None => String::new(),
    };

    let meeting_info = crate::worker::generate::build_meeting_info(&note);
    // 의도 기반 작업 모드 — 상위 에이전트가 발화 의도를 판정해 intent를 지정한다.
    // 강제로 구조를 바꾸지 않는다: 사용자가 구조화를 명시했을 때만 restructure.
    let mode_hint = match intent {
        "tidy" => "[작업 모드: 문체 정돈] 이미 적힌 항목의 구어체 어미만 개조식으로 다듬으세요. 새 소제목·글머리 구조를 만들거나 제목을 건드리지 마세요 — 사용자는 구조 변경이 아니라 말투 정돈만 원합니다.",
        "restructure" => "[작업 모드: 구조 재구성] 관련 항목을 소제목(<h2>)과 글머리(<ul><li>)로 묶어 문서 위계를 만드세요. 맨 위 제목 줄은 <h1>으로 둡니다 — 단 **기존 첫 줄이 제목이 아니라 단순 항목(내용 한 줄)이면 그 줄을 그대로 h1으로 올리지 말고, 전체를 아우르는 짧은 제목을 새로 짓습니다.** 어떤 항목이 섹션 <li>로 들어가면 같은 내용을 h1 제목에 중복으로 남기지 마세요(h1과 li에 같은 문장이 두 번 나오면 안 됨). 중복은 합치되 정보는 보존하세요.",
        _ => "[작업 모드: 받아적기] 새 내용을 기존 본문에 자연스럽게 이어 적으세요. 기존 항목은 그대로 유지합니다.",
    };
    let body_content = if current_html.trim().is_empty() {
        format!(
            "[노트 정보]\n{}\n\n[사용자 입력 — 이 내용으로 노트를 시작]\n{}",
            meeting_info.trim(),
            user_request.trim()
        )
    } else {
        format!(
            "[노트 정보]\n{}\n\n[현재 노트 본문 — 유지하고 여기에 이어 적거나 통합]\n{}\n\n[사용자 새 입력]\n{}",
            meeting_info.trim(),
            current_html,
            user_request.trim()
        )
    };
    let user_content = format!("{mode_hint}\n\n{body_content}");

    let result = ai::chat_completion(&llm, FREEFORM_WRITE_SYSTEM_PROMPT, &user_content).await?;
    let html = result.content;
    if html.trim().is_empty() {
        return Err(Error::Other("LLM이 빈 본문을 반환했습니다.".into()));
    }

    persist_body(app, pool, note_id, &note, &active, &html).await
}

/// 새 본문 HTML을 파일로 저장하고, 제목 동기화 + 버전 누적(archive→complete)까지 한다.
/// run_write와 map-reduce(run_attachment_merge)가 공유한다. 새 body id 반환.
async fn persist_body(
    app: &AppHandle,
    pool: &DbPool,
    note_id: &str,
    note: &Note,
    active: &Option<NoteBody>,
    html: &str,
) -> Result<String> {
    let new_id = Uuid::new_v4().to_string();
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| Error::Other(format!("app_data_dir: {e}")))?
        .join("note_bodies")
        .join(&new_id)
        .join("content.html");
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, html.as_bytes()).await?;
    let path_str = path.to_string_lossy().to_string();

    // 본문 첫 줄을 노트 제목으로 동기화 — 제목은 본문에서 도출(read-only 메타).
    let _ = notes::update(
        pool,
        note_id,
        notes::UpdateNoteInput {
            title: Some(extract_title(html)),
            ..Default::default()
        },
    )
    .await;

    let ctx = crate::worker::generate::context_snapshot_json(note);

    // 기존 버전이 있으면 archive 후 새 완료 버전(이력 누적). G-VERSION 베이스라인 유지.
    let (initial_content, initial_ctx) = match active {
        Some(b) => (
            b.initial_content_path.clone().or_else(|| b.content_path.clone()),
            b.initial_context_snapshot
                .clone()
                .or_else(|| Some(b.context_snapshot.clone())),
        ),
        None => (None, None),
    };
    let transcript_id = active.as_ref().and_then(|b| b.transcript_id.as_deref());

    note_bodies::archive_and_create_completed(
        pool,
        &new_id,
        note_id,
        transcript_id,
        &path_str,
        &ctx,
        initial_content.as_deref(),
        initial_ctx.as_deref(),
        false,
    )
    .await?;

    Ok(new_id)
}

// ============================================================================
// map-reduce: 여러 음성 전사(+텍스트)를 노트에 반영 (첨부 전송 경로)
// ============================================================================

const MAP_DRAFT_SYSTEM_PROMPT: &str = "당신은 음성 전사 한 건을 노트로 옮기는 필기 도우미입니다. 주어진 전사를 읽고 구어체·말 더듬음·반복·군말을 걷어내, 핵심을 짧은 항목으로 정돈한 **노트 조각(HTML)**을 만드세요.\n- 사실·수치·날짜·고유명사·결정사항·할 일은 빠짐없이 보존합니다. 어미만 개조식 명사형('했어'→'함', '하기로'→'~하기')으로 다듬고 정보는 잃지 마세요.\n- 한 전사는 보통 한 주제이니, 맨 위에 그 주제를 나타내는 짧은 소제목 <h2> 한 줄을 달고 아래에 <p>나 <ul><li>로 항목을 적습니다.\n- 스타일(색·폰트·style 속성) 금지, <html>/<head>/<body>/<style> 없이 **콘텐츠 조각만**(<h2>,<p>,<ul><li>,<table>).\n- 당신 자신의 멘트('정리했습니다','반영' 등) 금지. 언어는 전사 언어를 따릅니다.";

const MERGE_SYSTEM_PROMPT: &str = "당신은 여러 노트 조각을 하나의 노트로 통합하는 편집자입니다. [기존 노트]와 [새 초안]들, 그리고 [사용자 지시]가 주어집니다.\n- **모든 입력의 내용을 빠짐없이 보존**하면서 하나의 노트로 합치세요. 어떤 입력의 정보라도 누락하면 실패입니다 — 기존 노트 내용도 절대 버리지 마세요(기존 노트는 통합 대상 입력 중 하나일 뿐입니다).\n- 같은 주제는 한 섹션으로 합치되 같은 문장을 두 번 남기지 말고, 서로 다른 주제는 각각 <h2> 섹션으로 둡니다.\n- **맨 위 첫 줄은 노트 전체 제목이며 반드시 `<h1>제목</h1>` 한 줄로 답니다(평문·<p>·<h2>로 두지 마세요 — 제목 디자인이 붙으려면 <h1>이어야 합니다).** 여러 주제가 섞여 있으면 그 모두를 아우르는 제목을 새로 짓습니다 — 특정 한 주제의 제목만 쓰면 안 됩니다. 기존 노트 제목이 전체를 대표하면 그대로 <h1>로 유지하고, 새 주제가 늘어 안 맞으면 전체를 포괄하도록 다시 짓습니다.\n- [사용자 지시]가 있으면 그 구성 의도(예: '다른 주제로 분리','이어서','요약')를 따르되, 내용 보존이 항상 우선입니다. 지시가 없으면 주제 연속성으로 통합/분리를 판단합니다.\n- 스타일/style 속성 금지, 콘텐츠 조각만(맨 위 <h1> 제목 한 줄 + 그 아래 <h2>/<p>/<ul><li>/<table>). 당신 자신의 멘트 금지. 언어는 입력 언어를 따릅니다.";

/// map-reduce 본문 작성(첨부 전송): 각 전사를 독립 초안으로 정리(map)한 뒤, 기존 노트와
/// 초안들을 하나로 통합(reduce)한다. 기존 노트도 '입력 중 하나'로 다뤄 덮어쓰기로 인한
/// 손실을 막고, 주제가 다른 여러 녹음·대형 데이터에도 대응한다. 새 body id 반환.
pub async fn run_map_reduce(
    app: &AppHandle,
    pool: &DbPool,
    note_id: &str,
    user_instruction: &str,
    transcripts: &[String],
) -> Result<String> {
    let llm = ai_endpoints::get_active(pool, "llm")
        .await?
        .ok_or_else(|| Error::Other("활성 LLM endpoint가 없습니다. 설정에서 등록하세요.".into()))?;
    let note = notes::get(pool, note_id).await?;
    let active = note_bodies::get_active(pool, note_id).await?;
    let current_html = match &active {
        Some(b) => match &b.content_path {
            Some(p) => tokio::fs::read_to_string(p).await.unwrap_or_default(),
            None => String::new(),
        },
        None => String::new(),
    };

    // ── Map: 각 전사 → 독립 초안 ──
    let total = transcripts.len();
    let mut drafts: Vec<String> = Vec::new();
    for (i, tx) in transcripts.iter().enumerate() {
        if tx.trim().is_empty() {
            continue;
        }
        let _ = app.emit(
            "chat:status",
            serde_json::json!({ "note_id": note_id, "state": "drafting", "current": i + 1, "total": total }),
        );
        let draft = ai::chat_completion(&llm, MAP_DRAFT_SYSTEM_PROMPT, tx.trim()).await?;
        if !draft.content.trim().is_empty() {
            drafts.push(draft.content);
        }
    }
    if drafts.is_empty() {
        return Err(Error::Other("전사에서 노트 초안을 만들지 못했습니다.".into()));
    }

    // ── Reduce: 기존 노트 + 초안들 → 통합(항상 거쳐 <h1> 제목·일관 구조 보장) ──
    let _ = app.emit(
        "chat:status",
        serde_json::json!({ "note_id": note_id, "state": "merging" }),
    );
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!(
        "[기존 노트]\n{}",
        if current_html.trim().is_empty() {
            "(빈 노트)"
        } else {
            current_html.trim()
        }
    ));
    for (i, d) in drafts.iter().enumerate() {
        parts.push(format!("[새 초안 {}]\n{}", i + 1, d.trim()));
    }
    parts.push(format!(
        "[사용자 지시]\n{}",
        if user_instruction.trim().is_empty() {
            "(없음 — 주제에 따라 적절히 통합/분리)"
        } else {
            user_instruction.trim()
        }
    ));
    let merge_input = parts.join("\n\n");
    let merged = ai::chat_completion(&llm, MERGE_SYSTEM_PROMPT, &merge_input).await?;
    if merged.content.trim().is_empty() {
        return Err(Error::Other("통합 결과가 비어 있습니다.".into()));
    }

    persist_body(app, pool, note_id, &note, &active, &merged.content).await
}
