//! Headless E2E for the Phase 2 ASR→LLM chain against the *real* registered
//! endpoints + a real recording webm. Not part of the normal suite (#[ignore]):
//!
//!   ECHO_E2E_WEBM="<path.webm>" cargo test --lib e2e_real_chain -- --ignored --nocapture
//!
//! Exercises asr::split_to_wav_chunks + asr::asr_chunk (real ASR HTTP, both
//! branches via audio_format) + ai::chat_completion (normalizer + minutes) —
//! everything in transcribe/generate except the DB/worker glue.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::json;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::ai;
use crate::asr;
use crate::models::AiEndpoint;
use crate::prompts;

fn db_path() -> PathBuf {
    if let Ok(p) = std::env::var("ECHO_DB") {
        return PathBuf::from(p);
    }
    let appdata = std::env::var("APPDATA").expect("APPDATA or ECHO_DB must be set");
    PathBuf::from(appdata).join("com.echo.app").join("echo.db")
}

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "hits the real registered ASR/LLM endpoints; run with --ignored"]
async fn e2e_real_chain() {
    let opts = SqliteConnectOptions::new()
        .filename(db_path())
        .read_only(true);
    let pool = SqlitePool::connect_with(opts).await.expect("open echo.db");

    let asr_ep: AiEndpoint =
        sqlx::query_as("SELECT * FROM ai_endpoints WHERE kind = 'asr' AND is_active = 1 LIMIT 1")
            .fetch_one(&pool)
            .await
            .expect("an active ASR endpoint (activate one in Settings)");
    let llm_ep: AiEndpoint =
        sqlx::query_as("SELECT * FROM ai_endpoints WHERE kind = 'llm' AND is_active = 1 LIMIT 1")
            .fetch_one(&pool)
            .await
            .expect("an active LLM endpoint");
    eprintln!(
        "[e2e] ASR='{}' mode={} model={} | LLM='{}' model={}",
        asr_ep.name, asr_ep.request_mode, asr_ep.model_id, llm_ep.name, llm_ep.model_id
    );

    let webm = std::env::var("ECHO_E2E_WEBM").expect("set ECHO_E2E_WEBM to a recording .webm");
    let chunk_seconds = asr_ep.chunk_seconds.unwrap_or(300).max(1) as u32;
    let max_tokens = asr_ep.max_tokens.unwrap_or(4096);

    let out_dir = std::env::temp_dir().join("echo_e2e_chunks");
    let _ = std::fs::remove_dir_all(&out_dir);
    let chunks = asr::split_to_wav_chunks(Path::new(&webm), &out_dir, chunk_seconds)
        .await
        .expect("ffmpeg chunk split");
    eprintln!("[e2e] {} chunk(s)", chunks.len());
    assert!(!chunks.is_empty(), "no chunks produced");

    let mut texts: Vec<String> = Vec::new();
    for (i, c) in chunks.iter().enumerate() {
        let wav = tokio::fs::read(c).await.expect("read chunk");
        let dur = asr::wav_duration_secs(wav.len());
        let raw = asr::asr_chunk(&asr_ep, &asr_ep.request_mode, &wav, dur, "auto", max_tokens)
            .await
            .expect("asr_chunk HTTP");
        eprintln!(
            "[e2e] chunk {i}: {:?} chars (dur {:.1}s)",
            raw.as_ref().map(|s| s.len()),
            dur
        );
        if let Some(r) = raw {
            // eb0b667 — no post-process; push raw ASR output directly.
            texts.push(r);
        }
    }

    let transcript = texts.join("\n\n");
    eprintln!(
        "\n[e2e] ===== TRANSCRIPT ({} chars) =====\n{}\n",
        transcript.len(),
        clip(&transcript, 1000)
    );
    assert!(!transcript.trim().is_empty(), "ASR produced no transcript");

    let minutes = ai::chat_completion(
        &llm_ep,
        &prompts::minutes_system_prompt("ko"),
        &format!("[Transcript]\n{transcript}"),
    )
    .await
    .expect("minutes generation");
    eprintln!(
        "\n[e2e] ===== MINUTES ({} chars) =====\n{}\n",
        minutes.content.len(),
        clip(&minutes.content, 1500)
    );
    // 마크다운 전환 — 본문은 `#`/`##` 헤딩과 `- ` 불릿의 순수 마크다운이어야 한다.
    assert!(
        minutes.content.contains("# ") && !minutes.content.trim_start().starts_with('<'),
        "minutes output is not markdown"
    );

    let _ = std::fs::remove_dir_all(&out_dir);
}

/// done-stage 합성 컨텍스트 (활성 완료 본문 1개 → stage=done).
fn synthetic_done_bodies() -> Vec<crate::models::NoteBody> {
    vec![crate::models::NoteBody {
        id: "b".into(),
        note_id: "n".into(),
        transcript_id: None,
        content_path: None,
        status: "completed".into(),
        task_id: None,
        context_snapshot: "{}".into(),
        initial_content_path: None,
        initial_context_snapshot: None,
        archived: 0,
        is_manual_edit: 0,
        refine_request: None,
        created_at: String::new(),
        updated_at: String::new(),
    }]
}

/// Chat agent tool-selection oracle against the real LLM (2차 싱크 도구 세트).
/// Builds a done-stage system prompt + tool specs and checks the model picks an
/// acceptable tool per utterance (incl. adversarial no-tool cases).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "hits the real LLM endpoint; run with --ignored"]
async fn e2e_chat_tool_selection() {
    use crate::chat::{prompt, tools};

    let opts = SqliteConnectOptions::new()
        .filename(db_path())
        .read_only(true);
    let pool = SqlitePool::connect_with(opts).await.expect("open echo.db");
    let llm: AiEndpoint =
        sqlx::query_as("SELECT * FROM ai_endpoints WHERE kind = 'llm' AND is_active = 1 LIMIT 1")
            .fetch_one(&pool)
            .await
            .expect("an active LLM endpoint");
    eprintln!("[chat-e2e] LLM='{}' model={}", llm.name, llm.model_id);

    let bodies = synthetic_done_bodies();
    let none: Option<String> = None;
    let ctx = prompt::PromptCtx {
        note_started_at: &none,
        note_location: &none,
        note_language: "auto",
        note_theme: "default",
        recordings: &[],
        transcripts: &[],
        bodies: &bodies,
        timeline: &[],
        user_state: None,
        response_lang: "ko",
        note_type: None,
    };
    let system = prompt::build_system_prompt(&ctx);
    let tool_specs = tools::tools_for("done", &[]);
    eprintln!("[chat-e2e] stage=done, {} tools exposed", tool_specs.len());

    // (utterance, acceptable tool names; empty = expect NO tool call)
    // 본문이 프롬프트에 없으므로 내용 편집·질문은 read_minutes 선행이 정답
    // (edit_minutes 직행도 편집 의도로는 정답으로 인정).
    let scenarios: &[(&str, &[&str])] = &[
        (
            "제목을 분기 OKR 검토로 바꿔줘",
            &["read_minutes", "edit_minutes"],
        ),
        ("결정사항 부분 강조해줘", &["read_minutes", "edit_minutes"]),
        ("디자인 컬러풀하게 바꿔줘", &[]), // 회의록형은 고정 테마 — 안내로 거절, 무도구
        ("전사록 원문 그대로 보여줘", &["read_transcript"]),
        ("다시 전사해줘", &["ask_user"]), // 파괴적 — 확인부터
        ("방금 잘 처리된 거야?", &[]),    // adversarial — status question, no tool
    ];

    let mut pass = 0usize;
    for (utterance, expected) in scenarios {
        let messages = vec![
            json!({ "role": "system", "content": system }),
            json!({ "role": "user", "content": utterance }),
        ];
        let turn = ai::chat_with_tools(&llm, &messages, &tool_specs)
            .await
            .expect("chat_with_tools");
        let got: Vec<&str> = turn.tool_calls.iter().map(|t| t.name.as_str()).collect();
        let ok = if expected.is_empty() {
            got.is_empty()
        } else {
            got.iter().any(|g| expected.contains(g))
        };
        eprintln!(
            "[chat-e2e] {:?} → tools={:?} (expect one of {:?}) {}",
            utterance,
            got,
            expected,
            if ok { "PASS" } else { "FAIL" }
        );
        if ok {
            pass += 1;
        }
    }
    eprintln!("[chat-e2e] {}/{} scenarios matched", pass, scenarios.len());
    assert!(
        pass == scenarios.len(),
        "tool selection oracle: only {}/{} matched",
        pass,
        scenarios.len()
    );
}

/// Chat agent *behaviors* against the real LLM (2차 싱크): 내용 Q&A 는
/// read_minutes 를 선행하고(본문이 프롬프트에 없음), read 결과를 받은 다음 턴에
/// 본문 근거로 답하며, 편집 요청은 read → edit_minutes(str_replace 인자)로
/// 이어진다. Prints real outputs so quality is inspectable.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "hits the real LLM endpoint; run with --ignored"]
async fn e2e_chat_behaviors() {
    use crate::chat::{prompt, tools};

    let opts = SqliteConnectOptions::new()
        .filename(db_path())
        .read_only(true);
    let pool = SqlitePool::connect_with(opts).await.expect("open echo.db");
    let llm: AiEndpoint =
        sqlx::query_as("SELECT * FROM ai_endpoints WHERE kind = 'llm' AND is_active = 1 LIMIT 1")
            .fetch_one(&pool)
            .await
            .expect("an active LLM endpoint");

    let body_md = "# 제품 회의\n\n2026-05-26\n\n## 1. 신규 기능\n\n- AI 요약 기능 도입 결정함\n- 베타는 다음 주 시작 예정\n- 담당은 김개발로 정함\n\n## 2. 일정\n\n- 출시 목표 6월 말로 합의함\n- QA 기간 2주 확보 필요\n";

    let bodies = synthetic_done_bodies();
    let none: Option<String> = None;
    let ctx = prompt::PromptCtx {
        note_started_at: &none,
        note_location: &none,
        note_language: "auto",
        note_theme: "default",
        recordings: &[],
        transcripts: &[],
        bodies: &bodies,
        timeline: &[],
        user_state: None,
        response_lang: "ko",
        note_type: None,
    };
    let system = prompt::build_system_prompt(&ctx);
    let done_tools = tools::tools_for("done", &[]);

    // --- 1) 내용 Q&A: read_minutes 선행 → read 결과 주입 → 본문 근거 답변 ---
    let mut messages = vec![
        json!({"role":"system","content": system}),
        json!({"role":"user","content":"신규 기능 담당이 누구야?"}),
    ];
    let turn1 = ai::chat_with_tools(&llm, &messages, &done_tools)
        .await
        .expect("qa turn1");
    let t1_tools: Vec<&str> = turn1.tool_calls.iter().map(|t| t.name.as_str()).collect();
    eprintln!("[qa] turn1 tools={t1_tools:?}");
    assert!(
        t1_tools.contains(&"read_minutes"),
        "content Q&A must read_minutes first (body is not inlined), got {t1_tools:?}"
    );
    let rm = turn1
        .tool_calls
        .iter()
        .find(|t| t.name == "read_minutes")
        .unwrap();
    messages.push(json!({
        "role": "assistant",
        "content": if turn1.content.is_empty() { serde_json::Value::Null } else { json!(turn1.content) },
        "tool_calls": [{ "id": rm.id, "type": "function",
            "function": { "name": "read_minutes", "arguments": "{}" } }],
    }));
    messages.push(json!({
        "role": "tool", "tool_call_id": rm.id,
        "content": json!({ "ok": true, "content": body_md, "version_id": "fixture-v1" }).to_string(),
    }));
    let turn2 = ai::chat_with_tools(&llm, &messages, &done_tools)
        .await
        .expect("qa turn2");
    eprintln!("[qa] turn2 answer: {}\n", clip(&turn2.content, 300));
    assert!(
        turn2.content.contains("김개발"),
        "Q&A should surface the answer (김개발) from the read body"
    );

    // --- 2) 편집: read 결과를 본 뒤 edit_minutes(old/new) 로 이어지는가 ---
    let edit_msgs = vec![
        json!({"role":"system","content": messages[0]["content"]}),
        json!({"role":"user","content":"담당을 김개발이 아니라 박개발로 고쳐줘"}),
        json!({
            "role": "assistant", "content": serde_json::Value::Null,
            "tool_calls": [{ "id": "call_rm", "type": "function",
                "function": { "name": "read_minutes", "arguments": "{}" } }],
        }),
        json!({
            "role": "tool", "tool_call_id": "call_rm",
            "content": json!({ "ok": true, "content": body_md, "version_id": "fixture-v1" }).to_string(),
        }),
    ];
    let edit_turn = ai::chat_with_tools(&llm, &edit_msgs, &done_tools)
        .await
        .expect("edit turn");
    let e_tools: Vec<&str> = edit_turn
        .tool_calls
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    eprintln!("[edit] tools={e_tools:?}");
    let em = edit_turn
        .tool_calls
        .iter()
        .find(|t| t.name == "edit_minutes")
        .expect("edit request should call edit_minutes after read");
    let edits = em.args["edits"].as_array().cloned().unwrap_or_default();
    eprintln!("[edit] edits={}", serde_json::to_string(&edits).unwrap());
    let (new_content, diffs, errors) = crate::chat::edit::apply_str_edits(body_md, &edits);
    eprintln!("[edit] diffs={} errors={errors:?}", diffs.len());
    assert!(errors.is_empty(), "model edits failed to apply: {errors:?}");
    assert!(
        new_content.contains("박개발") && !new_content.contains("담당은 김개발"),
        "edit did not change 김개발→박개발"
    );
    eprintln!("[chat-behaviors] read→answer + read→edit assertions passed");
}

/// freeform 루프 오라클 — 실사고 재현 케이스: "'X:' 문구를 빼줘"는 그 문구*만*
/// 제거해야 하고(줄 삭제 금지), 새 내용은 write_note, 잡담은 무도구.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "hits the real LLM endpoint; run with --ignored"]
async fn e2e_freeform_behaviors() {
    use crate::chat::{prompt, tools};

    let opts = SqliteConnectOptions::new()
        .filename(db_path())
        .read_only(true);
    let pool = SqlitePool::connect_with(opts).await.expect("open echo.db");
    let llm: AiEndpoint =
        sqlx::query_as("SELECT * FROM ai_endpoints WHERE kind = 'llm' AND is_active = 1 LIMIT 1")
            .fetch_one(&pool)
            .await
            .expect("an active LLM endpoint");

    let body_md = "# 트럼프 행정부 관세 무효 판결 관련 법적 쟁점\n\n## 미국 무역법원의 트럼프 관세 무효 판결\n\n- **법적 근거 및 쟁점**:\n    - 트럼프 행정부의 근거: 무역법 122조(심각한 국제 수지 적자 시 관세 부과 가능)\n    - 법원 판단: 해당 조항을 적용한 관세는 불법적(적용 불가)\n";

    let bodies = synthetic_done_bodies();
    let none: Option<String> = None;
    let ctx = prompt::PromptCtx {
        note_started_at: &none,
        note_location: &none,
        note_language: "auto",
        note_theme: "notepad",
        recordings: &[],
        transcripts: &[],
        bodies: &bodies,
        timeline: &[],
        user_state: None,
        response_lang: "ko",
        note_type: Some("freeform"),
    };
    let system = prompt::build_system_prompt(&ctx);
    let ff_tools = tools::tools_for("freeform", &[]);
    eprintln!("[ff-e2e] {} tools exposed", ff_tools.len());

    // --- 1) 문구 삭제: read 결과 주입 후 edit_minutes 가 그 문구만 제거하는가 ---
    let msgs = vec![
        json!({"role":"system","content": system}),
        json!({"role":"user","content":"트럼프 행정부의 근거: <- 이 문구를 빼줘"}),
        json!({
            "role": "assistant", "content": serde_json::Value::Null,
            "tool_calls": [{ "id": "call_rm", "type": "function",
                "function": { "name": "read_minutes", "arguments": "{}" } }],
        }),
        json!({
            "role": "tool", "tool_call_id": "call_rm",
            "content": json!({ "ok": true, "content": body_md, "version_id": "fixture-v1" }).to_string(),
        }),
    ];
    let turn = ai::chat_with_tools(&llm, &msgs, &ff_tools)
        .await
        .expect("edit turn");
    let names: Vec<&str> = turn.tool_calls.iter().map(|t| t.name.as_str()).collect();
    eprintln!("[ff-e2e] phrase-delete tools={names:?}");
    let em = turn
        .tool_calls
        .iter()
        .find(|t| t.name == "edit_minutes")
        .expect("phrase deletion must go through edit_minutes");
    let edits = em.args["edits"].as_array().cloned().unwrap_or_default();
    eprintln!("[ff-e2e] edits={}", serde_json::to_string(&edits).unwrap());
    let (new_content, _diffs, errors) = crate::chat::edit::apply_str_edits(body_md, &edits);
    assert!(errors.is_empty(), "edits failed to apply: {errors:?}");
    assert!(
        new_content.contains("무역법 122조(심각한 국제 수지 적자 시 관세 부과 가능)"),
        "줄 내용(무역법 122조)이 보존돼야 함 — 줄 전체 삭제 금지:\n{new_content}"
    );
    assert!(
        !new_content.contains("트럼프 행정부의 근거:"),
        "요청한 문구는 제거돼야 함:\n{new_content}"
    );
    assert!(
        new_content.contains("법원 판단"),
        "다른 줄은 그대로여야 함:\n{new_content}"
    );

    // --- 1.5) 정정 방향 (실사고 재현): "A야, B가 아니라" — 본문에 있는 B가 old,
    //     사용자가 맞다고 한 A가 new. 발화 순서로 뒤집으면 안 된다. ---
    let body2 = "# 과학을 보다\n\n- 출연자:\n    - 김범준 (성균관대학교 물리학과)\n    - 최홍배 (세종대학교, 은하 연구/우주 먼지)\n    - 장홍재 (강원대학교 화학과)\n";
    let msgs2 = vec![
        json!({"role":"system","content": msgs[0]["content"]}),
        json!({"role":"user","content":"지웅배야 최홍배가아니라"}),
        json!({
            "role": "assistant", "content": serde_json::Value::Null,
            "tool_calls": [{ "id": "call_rm2", "type": "function",
                "function": { "name": "read_minutes", "arguments": "{}" } }],
        }),
        json!({
            "role": "tool", "tool_call_id": "call_rm2",
            "content": json!({ "ok": true, "content": body2, "version_id": "fixture-v1" }).to_string(),
        }),
    ];
    let turn2 = ai::chat_with_tools(&llm, &msgs2, &ff_tools)
        .await
        .expect("correction turn");
    let em2 = turn2
        .tool_calls
        .iter()
        .find(|t| t.name == "edit_minutes")
        .expect("name correction must call edit_minutes");
    let edits2 = em2.args["edits"].as_array().cloned().unwrap_or_default();
    eprintln!(
        "[ff-e2e] correction edits={}",
        serde_json::to_string(&edits2).unwrap()
    );
    let (fixed2, _d2, errs2) = crate::chat::edit::apply_str_edits(body2, &edits2);
    assert!(
        errs2.is_empty(),
        "correction edits failed to apply: {errs2:?}"
    );
    assert!(
        fixed2.contains("지웅배") && !fixed2.contains("최홍배"),
        "정정 방향 오류 — 본문의 최홍배가 지웅배로 바뀌어야 함:\n{fixed2}"
    );

    // --- 2) 새 내용 → write_note / 잡담 → 무도구 ---
    let cases: &[(&str, &[&str])] = &[
        ("다음 회의는 수요일 3시로 확정", &["write_note"]),
        ("아 배고프다 점심 뭐먹지", &[]),
    ];
    for (utterance, expected) in cases {
        let msgs = vec![
            json!({"role":"system","content": msgs[0]["content"]}),
            json!({"role":"user","content": utterance}),
        ];
        let t2 = ai::chat_with_tools(&llm, &msgs, &ff_tools)
            .await
            .expect("turn");
        let got: Vec<&str> = t2.tool_calls.iter().map(|t| t.name.as_str()).collect();
        let ok = if expected.is_empty() {
            got.is_empty()
        } else {
            got.iter().any(|g| expected.contains(g))
        };
        eprintln!(
            "[ff-e2e] {:?} → {:?} (expect {:?}) {}",
            utterance,
            got,
            expected,
            if ok { "PASS" } else { "FAIL" }
        );
        assert!(ok, "freeform routing failed for {utterance:?}");
    }
    eprintln!("[ff-e2e] freeform behaviors passed");
}

/// Seed a done-stage test note (recording + completed transcript + completed
/// body) directly into echo.db so the chat refine UI can be tested. Writes the
/// transcript/body files under app_data too.
///
///   ECHO_DB="<echo.db>" ECHO_E2E_WEBM="<path.webm>" cargo test --lib seed_test_note -- --ignored --nocapture
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "mutates echo.db; run explicitly with --ignored"]
async fn seed_test_note() {
    let webm = std::env::var("ECHO_E2E_WEBM").expect("set ECHO_E2E_WEBM to a recording .webm");
    assert!(Path::new(&webm).is_file(), "ECHO_E2E_WEBM must name an existing file");
    let db = db_path();
    let app_data = db.parent().expect("db parent").to_path_buf();
    let opts = SqliteConnectOptions::new()
        .filename(&db)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePool::connect_with(opts)
        .await
        .expect("open echo.db (rw)");

    let note_id = Uuid::new_v4().to_string();
    let rec_id = Uuid::new_v4().to_string();
    let transcript_id = Uuid::new_v4().to_string();
    let body_id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO notes (id, title, language, started_at, source_type) \
         VALUES (?, ?, 'kor', datetime('now'), 'audio')",
    )
    .bind(&note_id)
    .bind("테스트 노트 — refine 검증")
    .execute(&pool)
    .await
    .expect("insert note");

    // Recording row points at an existing webm (download/retry realism).
    sqlx::query(
        "INSERT INTO recordings (id, note_id, file_path, original_filename, format, finalized_at) \
         VALUES (?, ?, ?, 'recording.webm', 'webm', datetime('now'))",
    )
    .bind(&rec_id)
    .bind(&note_id)
    .bind(webm)
    .execute(&pool)
    .await
    .expect("insert recording");

    // Transcript file + completed row.
    let transcript_text = "오늘 제품 출시 준비 회의 시작하겠습니다.\n\n신규 기능 범위인데요, AI 자동 요약 기능은 이번에 넣는 걸로 결정했고요.\n\n실시간 협업 편집은 일정상 이번 출시에서는 빼고 다음 분기에 다시 보기로 했습니다.\n\n모바일은 일단 반응형으로 우선 대응하죠.\n\n일정은 6월 말 출시 목표로 가고, QA는 최소 2주는 확보해야 합니다.\n\n프론트 인력 한 명 더 충원하는 건 논의는 했는데 결론은 아직 안 났어요.\n\n리스크로는 외부 ASR API 비용이 갑자기 늘 수 있어서 사용량 모니터링 대시보드가 필요하고요.\n\n개인정보 처리방침 업데이트는 법무 검토 대기 중입니다.\n\n후속으로 김개발님이 QA 일정 수립 6월 2일까지, 박기획님이 개인정보 처리방침 법무 검토 요청 금주 내로 해주시기로 했습니다.";
    let tdir = app_data.join("transcripts").join(&transcript_id);
    std::fs::create_dir_all(&tdir).expect("mk transcript dir");
    let tpath = tdir.join("raw.txt");
    std::fs::write(&tpath, transcript_text).expect("write transcript");
    let tpath_str = tpath.to_string_lossy().to_string();
    sqlx::query(
        "INSERT INTO transcripts (id, note_id, recording_id, raw_path, corrected_path, status) \
         VALUES (?, ?, ?, ?, ?, 'completed')",
    )
    .bind(&transcript_id)
    .bind(&note_id)
    .bind(&rec_id)
    .bind(&tpath_str)
    .bind(&tpath_str)
    .execute(&pool)
    .await
    .expect("insert transcript");

    // Body file + completed row → done stage. 마크다운 본문(디자인은 테마가 담당).
    let body_md = r##"# 제품 출시 준비 회의

2026-05-26

## 1. 신규 기능 범위

- AI 자동 요약 기능 도입 결정함
- 실시간 협업 편집은 이번 출시 범위에서 제외, 다음 분기 검토 예정
- 모바일은 반응형으로 우선 대응하기로 함

## 2. 일정 및 리소스

- 출시 목표 6월 말로 합의함
- QA 기간 최소 2주 확보 필요
- 프론트 인력 1명 추가 충원 논의됨 (결론 미정)

3. 리스크 섹션과 후속 조치는 아래 참고.

## 3. 리스크

- 외부 ASR API 비용 급증 가능성 → 사용량 모니터링 대시보드 필요
- 개인정보 처리방침 업데이트 법무 검토 대기 중

**결정**: AI 자동 요약 도입 / 실시간 협업 편집 보류, 6월 말 출시 목표 확정.

**후속**: 김개발 — QA 일정 수립(6/2까지), 박기획 — 개인정보 처리방침 법무 검토 요청(금주 내).
"##;
    let bpath_rel = crate::storage::body_rel(&note_id, &body_id, "md");
    let bpath = app_data.join(&bpath_rel);
    std::fs::create_dir_all(bpath.parent().unwrap()).expect("mk body dir");
    std::fs::write(&bpath, body_md).expect("write body");
    let bpath_str = bpath_rel;
    let ctx = json!({
        "title": "테스트 노트 — refine 검증",
        "description": null,
        "location": null,
        "language": "kor",
        "started_at": null
    })
    .to_string();
    sqlx::query(
        "INSERT INTO note_bodies (id, note_id, transcript_id, content_path, status, context_snapshot) \
         VALUES (?, ?, ?, ?, 'completed', ?)",
    )
    .bind(&body_id)
    .bind(&note_id)
    .bind(&transcript_id)
    .bind(&bpath_str)
    .bind(&ctx)
    .execute(&pool)
    .await
    .expect("insert note_body");

    eprintln!("[seed] done-stage note ready — note_id={note_id}");
    eprintln!("[seed] 앱의 노트 목록을 새로고침하면 '테스트 노트 — refine 검증' 이 보입니다.");
}

/// Manual-edit body versioning — the core of `save_manual_body_edit` (F-VIEW).
/// Self-contained (temp migrated DB, no real endpoints) so it runs in the normal
/// suite as a regression guard. Verifies the is_manual flag, the G-DB-004
/// one-active invariant across the archive+create, and G-VERSION-004 baseline
/// carry-forward.
#[tokio::test]
async fn manual_edit_creates_active_manual_version() {
    use crate::repo::note_bodies;

    let dir = std::env::temp_dir().join(format!("echo_manual_edit_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let pool = crate::db::init_pool(&dir.join("echo.db"))
        .await
        .expect("init temp db");

    let note_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO notes (id, title, language, started_at, source_type) \
         VALUES (?, '수동편집 테스트', 'kor', datetime('now'), 'audio')",
    )
    .bind(&note_id)
    .execute(&pool)
    .await
    .expect("insert note");

    // Seed one active, AI-generated (is_manual_edit=0) completed body.
    let orig_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO note_bodies (id, note_id, content_path, status, context_snapshot, initial_content_path) \
         VALUES (?, ?, '/tmp/orig.html', 'completed', '{}', '/tmp/orig.html')",
    )
    .bind(&orig_id)
    .bind(&note_id)
    .execute(&pool)
    .await
    .expect("insert active body");

    // Exercise the manual-edit core (what save_manual_body_edit calls).
    let new_id = Uuid::new_v4().to_string();
    note_bodies::archive_and_create_completed(
        &pool,
        &new_id,
        &note_id,
        None,
        "/tmp/edited.html",
        "{}",
        Some("/tmp/orig.html"),
        Some("{}"),
        true,
        None,
        Some(&orig_id),
    )
    .await
    .expect("manual edit archive+create");

    // G-DB-004 — exactly one active, and it's the new manual version.
    let active = note_bodies::get_active(&pool, &note_id)
        .await
        .unwrap()
        .expect("an active body");
    assert_eq!(active.id, new_id, "new manual version should be active");
    assert_eq!(
        active.is_manual_edit, 1,
        "manual version flagged is_manual_edit=1"
    );
    assert_eq!(active.archived, 0);
    assert_eq!(active.content_path.as_deref(), Some("/tmp/edited.html"));
    // G-VERSION-004 — stage-1 baseline carried forward.
    assert_eq!(
        active.initial_content_path.as_deref(),
        Some("/tmp/orig.html")
    );

    // Old AI body archived, still flagged non-manual.
    let all = note_bodies::list_for_note(&pool, &note_id).await.unwrap();
    assert_eq!(all.len(), 2, "archive+create yields exactly 2 rows");
    let orig = all.iter().find(|b| b.id == orig_id).unwrap();
    assert_eq!(orig.archived, 1, "original body archived");
    assert_eq!(orig.is_manual_edit, 0);

    pool.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}
