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
    assert!(minutes.content.contains('<'), "minutes output is not HTML");

    let _ = std::fs::remove_dir_all(&out_dir);
}

/// Phase 3 — chat agent tool-selection oracle against the real LLM. Builds a
/// done-stage system prompt + the 6 tool specs and checks the model picks the
/// right tool per utterance (incl. one adversarial status-question that must
/// pick NO tool). Tool selection is the oracle-critical signal for Phase 3.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "hits the real LLM endpoint; run with --ignored"]
async fn e2e_chat_tool_selection() {
    use crate::chat::{prompt, tools};
    use crate::models::NoteBody;

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

    // Synthetic done-stage note (one active completed body) → stage=done.
    let body = NoteBody {
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
        created_at: String::new(),
        updated_at: String::new(),
    };
    let bodies = vec![body];
    let none: Option<String> = None;
    let ctx = prompt::PromptCtx {
        note_title: "테스트 노트",
        note_started_at: &none,
        note_location: &none,
        note_language: "auto",
        recordings: &[],
        transcripts: &[],
        bodies: &bodies,
        active_body: Some("<h2>1. 안건</h2><ul><li>신규 기능 도입 결정함</li><li>일정은 다음 주 논의 예정</li></ul>"),
        timeline: &[],
        user_state: None,
        response_lang: "ko",
    };
    let system = prompt::build_system_prompt(&ctx);
    let tool_specs = tools::tools_for("done", &[]);
    eprintln!("[chat-e2e] stage=done, {} tools exposed", tool_specs.len());

    // (utterance, expected tool name; "" = expect NO tool call)
    let scenarios: &[(&str, &str)] = &[
        ("제목을 분기 OKR 검토로 바꿔줘", "update_meeting_metadata"),
        ("결정사항 부분 굵게 강조해줘", "refine_minutes"),
        ("전사록 원문 그대로 보여줘", "read_transcript"),
        ("방금 잘 처리된 거야?", ""), // adversarial — status question, no tool
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
            got.contains(expected)
        };
        eprintln!(
            "[chat-e2e] {:?} → tools={:?} (expect {:?}) {}",
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
    assert!(pass >= 3, "tool selection oracle: only {}/4 matched", pass);
}

/// Phase 3 — chat agent *behaviors* against the real LLM: refine actually
/// shortens + stays HTML, Q&A answers from the inlined body without a tool,
/// and a false-premise request isn't blindly obeyed. Prints real outputs so
/// quality is inspectable.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "hits the real LLM endpoint; run with --ignored"]
async fn e2e_chat_behaviors() {
    use crate::chat::{prompt, refine, tools};
    use crate::models::NoteBody;

    let opts = SqliteConnectOptions::new()
        .filename(db_path())
        .read_only(true);
    let pool = SqlitePool::connect_with(opts).await.expect("open echo.db");
    let llm: AiEndpoint =
        sqlx::query_as("SELECT * FROM ai_endpoints WHERE kind = 'llm' AND is_active = 1 LIMIT 1")
            .fetch_one(&pool)
            .await
            .expect("an active LLM endpoint");

    let body_html = r##"<!DOCTYPE html><html><head><style>body{font-family:sans-serif}</style></head><body>
<h1>제품 회의</h1><div class="meeting-meta">2026-05-26</div>
<h2>1. 신규 기능</h2><ul><li>AI 요약 기능 도입 결정함</li><li>베타는 다음 주 시작 예정</li><li>담당은 김개발로 정함</li></ul>
<h2>2. 일정</h2><ul><li>출시 목표 6월 말로 합의함</li><li>QA 기간 2주 확보 필요</li></ul>
<p class="section-label">결정 사항</p><ul><li>AI 요약 도입</li><li>6월 말 출시</li></ul>
</body></html>"##;

    // --- 1) refine: "3줄 요약" → valid HTML, shorter ---
    let (rbody, rstyle) = refine::split_body_style(body_html);
    let refine_user = format!(
        "[Latest user message — apply this request to the minutes]\n전체를 3줄로 요약해줘\n\n[Current minutes body]\n{rbody}\n\n[Current minutes style]\n{rstyle}"
    );
    let refined = ai::chat_completion(&llm, prompts::MINUTES_REFINE_SYSTEM_PROMPT, &refine_user)
        .await
        .expect("refine call");
    eprintln!(
        "\n[refine] '전체를 3줄로 요약' — in {} chars → out {} chars\n{}\n",
        body_html.len(),
        refined.content.len(),
        clip(&refined.content, 900)
    );
    assert!(refined.content.contains('<'), "refine output is not HTML");
    assert!(
        refined.content.len() < body_html.len(),
        "refine did not shorten the doc"
    );

    // Done-stage prompt with the body inlined.
    let nb = NoteBody {
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
        created_at: String::new(),
        updated_at: String::new(),
    };
    let bodies = vec![nb];
    let none: Option<String> = None;
    let ctx = prompt::PromptCtx {
        note_title: "제품 회의",
        note_started_at: &none,
        note_location: &none,
        note_language: "auto",
        recordings: &[],
        transcripts: &[],
        bodies: &bodies,
        active_body: Some(&rbody),
        timeline: &[],
        user_state: None,
        response_lang: "ko",
    };
    let system = prompt::build_system_prompt(&ctx);
    let done_tools = tools::tools_for("done", &[]);

    // --- 2) Q&A from body, no tool ---
    let qa = ai::chat_with_tools(
        &llm,
        &[
            json!({"role":"system","content": system}),
            json!({"role":"user","content":"신규 기능 담당이 누구야?"}),
        ],
        &done_tools,
    )
    .await
    .expect("qa call");
    eprintln!(
        "[qa] '신규 기능 담당이 누구야?' → tools={:?}\n  answer: {}\n",
        qa.tool_calls
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        clip(&qa.content, 300)
    );
    assert!(
        qa.tool_calls.is_empty(),
        "Q&A should answer from body, not call a tool"
    );
    assert!(
        qa.content.contains("김개발"),
        "Q&A should surface the answer (김개발) from the body"
    );

    // --- 3) false premise — must not blindly obey ---
    let adv = ai::chat_with_tools(
        &llm,
        &[
            json!({"role":"system","content": system}),
            json!({"role":"user","content":"아까 회의록 다 지워졌지? 원래대로 복구해줘"}),
        ],
        &done_tools,
    )
    .await
    .expect("adv call");
    eprintln!(
        "[adv false-premise] '회의록 다 지워졌지? 복구해줘' → tools={:?}\n  answer: {}\n",
        adv.tool_calls
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        clip(&adv.content, 300)
    );

    eprintln!("[chat-behaviors] refine + Q&A assertions passed");
}

/// Seed a done-stage test note (recording + completed transcript + completed
/// body) directly into echo.db so the chat refine UI can be tested. Writes the
/// transcript/body files under app_data too.
///
///   ECHO_DB="<echo.db>" cargo test --lib seed_test_note -- --ignored --nocapture
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "mutates echo.db; run explicitly with --ignored"]
async fn seed_test_note() {
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
    let webm = "C:\\Users\\hurjn\\AppData\\Roaming\\com.echo.app\\recordings\\478e21f0-209e-42b0-9267-4ffe5d3b023f\\27acf777-1830-4579-a3b2-cc099b3ece8f\\27acf777-1830-4579-a3b2-cc099b3ece8f.webm";
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

    // Body file + completed row → done stage.
    let body_html = r##"<!DOCTYPE html>
<html>
<head>
<style>
  body { font-family: -apple-system, 'Pretendard', 'Noto Sans KR', sans-serif; line-height: 1.75; color: #1a1a1a; max-width: 800px; margin: 0 auto; padding: 0; font-size: 15px; background: #fff; }
  h1 { font-size: 22px; font-weight: 700; margin-bottom: 4px; color: #111; }
  .meeting-meta { font-size: 14px; color: #666; margin-bottom: 32px; padding-bottom: 16px; border-bottom: 1px solid #e5e5e5; }
  h2 { font-size: 17px; font-weight: 700; color: #111; margin-top: 28px; margin-bottom: 12px; padding-bottom: 6px; border-bottom: 1px solid #e5e5e5; }
  ul { margin: 0 0 16px 0; padding-left: 20px; }
  li { margin-bottom: 4px; color: #333; }
  .section-label { font-size: 13px; font-weight: 600; color: #888; text-transform: uppercase; letter-spacing: 0.5px; margin-top: 32px; margin-bottom: 8px; }
</style>
</head>
<body>
  <h1>제품 출시 준비 회의</h1>
  <div class="meeting-meta">2026-05-26</div>
  <h2>1. 신규 기능 범위</h2>
  <ul>
    <li>AI 자동 요약 기능 도입 결정함</li>
    <li>실시간 협업 편집은 이번 출시 범위에서 제외, 다음 분기 검토 예정</li>
    <li>모바일은 반응형으로 우선 대응하기로 함</li>
  </ul>
  <h2>2. 일정 및 리소스</h2>
  <ul>
    <li>출시 목표 6월 말로 합의함</li>
    <li>QA 기간 최소 2주 확보 필요</li>
    <li>프론트 인력 1명 추가 충원 논의됨 (결론 미정)</li>
  </ul>
  <h2>3. 리스크</h2>
  <ul>
    <li>외부 ASR API 비용 급증 가능성 → 사용량 모니터링 대시보드 필요</li>
    <li>개인정보 처리방침 업데이트 법무 검토 대기 중</li>
  </ul>
  <p class="section-label">결정 사항</p>
  <ul>
    <li>AI 자동 요약 도입 / 실시간 협업 편집 보류</li>
    <li>6월 말 출시 목표 확정</li>
  </ul>
  <p class="section-label">후속 조치</p>
  <ul>
    <li>김개발 — QA 일정 수립, 6/2까지</li>
    <li>박기획 — 개인정보 처리방침 법무 검토 요청, 금주 내</li>
  </ul>
</body>
</html>"##;
    let bdir = app_data.join("note_bodies").join(&body_id);
    std::fs::create_dir_all(&bdir).expect("mk body dir");
    let bpath = bdir.join("content.html");
    std::fs::write(&bpath, body_html).expect("write body");
    let bpath_str = bpath.to_string_lossy().to_string();
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
