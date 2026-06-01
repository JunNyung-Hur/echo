//! Chat-agent tool specs + stage/role/capability gating (Phase 3).
//!
//! 1:1 port of backend/app/services/chat_agent/tool_specs.py. The Korean tool
//! descriptions are preserved verbatim — the LLM's tool-selection behavior is
//! tuned to this exact wording and Phase 3 oracle parity depends on it. Tool
//! *names* (update_meeting_metadata / refine_minutes) are kept as-is, but the
//! Korean *wording* in descriptions is renamed 회의/회의록 → 노트 (Phase 4 — Second Brain).

#![allow(dead_code)] // Phase 3: wired by the agent loop (in progress).

use serde_json::{json, Value};

/// Stage → tool names exposed. update_meeting_metadata is in every stage;
/// refine_minutes / read_transcript only in `done`; recording-dependent tools
/// from `transcribing` on. (ports _STAGE_TOOLS)
fn stage_tools(stage: &str) -> Vec<&'static str> {
    match stage {
        // 노트 필기형 — 채팅으로 노트 본문을 작성/수정. 제목·메타는 본문에서 도출(메타 도구 없음).
        "freeform" => vec!["write_note"],
        // 메타(제목·시간·장소·언어)는 read-only — 에이전트가 못 바꾼다.
        // 제목=본문 첫 줄, 시간=노트 생성 시각 고정, 장소 없음, 언어=RecordGate(UI).
        "before" | "recording" => vec![],
        "transcribing" => vec![
            "get_recording_download_url",
            "retry_transcribe",
            "retry_failed_task",
        ],
        // `done` (and any unknown stage → full set)
        _ => vec![
            "refine_minutes",
            "get_recording_download_url",
            "retry_transcribe",
            "retry_failed_task",
            "read_transcript",
        ],
    }
}

/// Tools the LLM should see this turn. Stage gating + capability gating: any
/// tool whose UI action is `hidden` (in `hidden_tools`) is stripped so the LLM
/// can't trigger an action the user can't see. (ports tools_for)
pub fn tools_for(stage: &str, hidden_tools: &[String]) -> Vec<Value> {
    let allowed = stage_tools(stage);
    all_specs()
        .into_iter()
        .filter(|spec| {
            let name = spec["function"]["name"].as_str().unwrap_or("");
            allowed.contains(&name) && !hidden_tools.iter().any(|h| h == name)
        })
        .collect()
}

/// The 6 OpenAI function-calling tool specs.
pub fn all_specs() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "update_meeting_metadata",
                "description": "노트의 메타데이터(제목·장소·언어·시작 시각)를 변경합니다.\n**호출 전제 — 명시적 필드 신호 필수**: 사용자 발화에 변경 대상 *필드명*이 명시적으로 들어 있을 때만 호출. 인정되는 필드 신호 예: '제목/타이틀/이름' (title), '장소/위치' (location), '시간/시각/시작 시각' (started_at), '언어/한국어/영어' (language). **단순 'X로 바꿔줘' / 'X로 해줘' 류에서 X가 무슨 필드 값인지 *추론하지 말 것*** — 같은 표현이 노트 다듬기(refine_minutes) 의도일 수도 있고, X가 generic 단어('디자인', '노트' 등)면 어떤 필드로도 부적절. 필드 신호 없는 모호 발화는 호출 X, 사용자에게 짧게 되묻기 ('노트 제목을 바꾸시려는 건가요, 노트를 다듬으시려는 건가요?' 같은 식).\n**새 값 추출 룰** (필드 신호가 있는 경우): 발화에 새 값 후보(명사구·고유명사·시각 표현)가 명확히 있으면 그 후보를 새 값으로 즉시 호출. 정확한 값을 다시 물어보지 말 것. 예:\n- '제목 바꿔줘, 유튜브 채널 보다의 과학을보다 영상' → title='과학을보다' (제목 신호 + 영상명 후보). 즉시 호출.\n- '제목 X로 바꿔줘' → title='X'. 즉시 호출.\n- '장소를 서울 본사 회의실로 옮겨줘' → location='서울 본사 회의실'. 즉시 호출.\n- '세미나 정리 파일로 바꿔줘' → 필드 신호 없음. ❌ 자동 title 변경 금지. 노트 다듬기 의도인지 짧게 되묻기.\n- '디자인' (단일 generic 단어) → 필드 신호 없음, 값도 어떤 필드로도 부적절. ❌ 호출 금지.\n값 후보가 발화에 없거나 진짜 모호할 땐 사용자에게 되묻기. 단순 paraphrase 발화 ('제목 그대로 복사' 류)는 금지 — 사용자 발화 전체를 title 인자에 넣지 말고, 발화에서 *제목 후보로 보이는 명사구만* 추출해서 넣을 것. 빈 문자열이나 추측한 값으로 호출하지 마세요.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "새 노트 제목. **사용자 발화 전체를 그대로 복사 금지** — 발화에서 *제목 후보 명사구* (영상명·문서명·고유명사·핵심 주제 단어)만 추출해서 넣을 것. 다음은 명백한 오용 예:\n- 사용자: '유튜브 채널인 보다 채널의 과학을보다라는 영상 올릴꺼야 제목 바꿔줘' → BAD ❌ title='유튜브 채널인 보다 채널의 과학을보다라는 영상 올릴꺼야' (발화 그대로 복사). GOOD ✅ title='과학을보다' (영상명만 추출).\n- 사용자: '제목 분기 OKR 검토로 해줘' → GOOD ✅ title='분기 OKR 검토'.\n- 사용자: '제목 바꿔줘' (값 후보 없음) → 호출 X. 사용자에게 새 제목 한 번 짧게 되묻기."
                        },
                        "location": { "type": "string", "description": "새 장소. 빈 문자열로 보내면 장소를 비웁니다." },
                        "language": {
                            "type": "string",
                            "enum": ["auto", "kor", "eng"],
                            "description": "노트 언어 (전사/노트 작성용). auto=자동, kor=한국어, eng=English"
                        },
                        "started_at": {
                            "type": "string",
                            "description": "ISO 8601 형식의 노트 시작 시각 (예: '2026-05-15T14:00:00+09:00')"
                        }
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "refine_minutes",
                "description": "현재 노트를 사용자 요청대로 다시 만듭니다. 노트가 생성된(done) 상태에서만 의미. **done 단계에서 사용자가 노트를 *어떤 방식으로든* 손보고 싶다는 의도를 보이면 이 도구를 호출.** 포괄적 — 내용·구조·길이 변경, 시각 스타일·디자인·CSS 변경, 장르/포맷 전환(강의노트·보고서·메모 등), 본문 안 시각 요소 수정(구분선·굵게·표·헤더 등), 용어·사실 정정 모두 처리. **사용자의 원문을 거의 그대로 `user_request`에 담아 전달** — 카테고리 판단(내용/시각/장르)이나 본문 채널로 좁히기는 워커가 알아서 함. 메타데이터 변경 후 사용자가 노트 반영에 동의했을 때도 호출. 이 도구는 1-2분간 동기적으로 대기하고, 완료 후 새 노트 버전을 반환합니다.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "user_request": {
                            "type": "string",
                            "description": "사용자의 원문 요청을 거의 그대로 담을 것(말투만 정리). 카테고리로 분류하거나 본문 채널 예시로 좁히지 말 것 — 워커가 BODY/STYLE/장르를 알아서 라우팅함. 예: '디자인 바꿔줘' / '강의노트로 바꿔줘' / '결정사항 강조해줘' / '잡담 제거' / '제목 변경 사항을 노트에 반영' — 어떤 종류든 그대로 전달."
                        }
                    },
                    "required": ["user_request"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "write_note",
                "description": "노트 필기형에서, 사용자가 채팅으로 전달한 내용을 노트 본문에 작성·반영합니다. 사용자가 노트에 적고 싶은 내용을 말하거나 기존 노트를 고치고 싶어할 때 호출. 본문이 없으면 새로 작성하고, 있으면 요청대로 수정합니다. 사용자 발화를 거의 그대로 user_request에 담아 전달하세요. 1-2분간 동기로 대기 후 새 노트 본문을 반환합니다.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "user_request": { "type": "string", "description": "노트에 반영할 사용자 콘텐츠 원문만(말투만 정리). agent 자신의 약속·확인·안내 문구('앞으로 ~하겠습니다','수정할까요' 등)는 절대 넣지 말 것." },
                        "intent": { "type": "string", "enum": ["append", "tidy", "restructure"], "description": "사용자 의도. **append**=새 내용을 받아적기(기본). **tidy**=이미 적힌 내용의 말투·문체만 다듬기 — '정리해줘/다듬어줘'인데 구조를 바꾸라는 말은 없을 때(소제목·불릿·제목 손대지 않음). **restructure**=구조화를 명시적으로 요청할 때만 — '항목으로 묶어줘/소제목 달아줘/구조 잡아줘/개요 만들어줘'처럼. 사용자가 구조·스타일 변경을 명시하지 않았으면 restructure를 쓰지 말고 tidy(또는 append)로." }
                    },
                    "required": ["user_request"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "get_recording_download_url",
                "description": "녹음 파일 다운로드 URL을 생성합니다. 사용자가 녹음을 다운받고 싶다고 할 때 호출하세요. 녹음 파일이 없거나 14일 보존 정책으로 삭제된 경우 결과에 오류 사유가 담겨 옵니다.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "retry_transcribe",
                "description": "전사를 처음부터 다시 시도합니다. 기존 전사록과 노트가 모두 폐기되고 재생성됩니다 — 사용자가 이 비파괴 효과를 분명히 확인했을 때만 호출하세요. 이 도구는 작업을 시작만 시키고 즉시 반환합니다. 실제 완료까지 5-10분 정도 걸린다고 사용자에게 안내하세요.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "retry_failed_task",
                "description": "현재 실패한 작업(전사 또는 노트 생성)을 자동으로 재시작합니다. **호출 조건이 매우 엄격함**: 사용자가 *명시적으로* 재시도를 지시한 발화일 때만 호출. 예: '다시 시도해줘' / '재시도해줘' / '다시 해줘' / (이전 turn에서 'X를 다시 시도해드릴까요?'라고 물은 직후) '응' / '그래' / '해줘'. **호출 금지 케이스**: 사용자가 단순 상태 질문('뭐야?' / '잘 됐어?' / '어떻게 됐어?' / '끝났어?' 등)을 하거나 화면 안내만 요청한 경우. 이런 발화는 사용자가 *상황을 인지하고 싶은 단계*지 *동작을 시키는 단계*가 아님. 답으로 사실 안내 + '다시 시도해드릴까요?' 제안까지만 하고 도구는 호출하지 말 것. 외부 AI 서버 일시 장애는 즉시 재시도로 회복 안 될 수 있어서 동의 없는 자동 재시도는 무의미한 fail 누적 → UX 악화. 실패한 노트 생성이 있으면 노트만 재생성(전사록은 그대로, 1-2분 소요), 실패한 전사가 있으면 전사부터 재시작(기존 전사록·노트 폐기, 5-10분 소요). 결과의 `retried` 필드(`minutes` 또는 `transcript`)와 `eta_minutes`로 사용자에게 정확한 소요 시간을 안내하세요. 재시작할 작업이 없으면 결과에 오류 사유가 담겨 옵니다.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_transcript",
                "description": "전사록(녹음의 STT 결과 원문)을 가져와 사용자에게 보여주거나 깊은 Q&A에 활용합니다. **호출 조건이 매우 엄격함**: 사용자가 *명시적으로* 전사록 원문을 보거나 인용해달라고 한 경우에만 호출. 예: '전사록 보여줘' / '전사 읽어줘' / '전사록 그대로 좀 볼래' / '녹음에서 X 부분 원문 어떻게 말했어?'. **호출 금지 케이스 (매우 중요)**: (1) 사용자가 노트 내용에 대해 묻거나 정정·다듬기를 요청한 경우 — *활성 노트 내용이 이미 system prompt에 들어있음*. 그 내용으로 답하거나 `refine_minutes`로 처리. (2) 사용자가 노트만으로 충분히 답할 수 있는 일반 질문 ('결정사항 뭐였어?', '참석자 누구야?')을 한 경우. (3) **사용자가 명시적으로 묻기 전에 자발적으로 *제안*하지 말 것.** '전사록에서 확인해드릴까요?' / '전사 원문을 보여드릴까요?' 류의 선제 제안은 금지. 사용자가 자기 입으로 '전사 / 원문 / 받아쓰기' 같은 단어로 요청하지 않는 한 호출하지 않음. **응답 처리**: 도구 결과의 `content`는 백엔드가 자동으로 응답에 이어 붙여 사용자에게 보여줍니다. LLM은 짧은 안내 한 줄(예: '전사 원문이에요.') 정도만 호출 직전에 emit하면 충분. 도구 결과를 다시 그대로 출력하지 마세요 — 백엔드가 중복 없이 처리합니다.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
    ]
}
