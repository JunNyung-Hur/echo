//! Freeform intent stage.
//!
//! A single fast LLM call classifies a freeform *text* turn — is it note
//! content (and how to write it) or just conversation (greeting / smalltalk /
//! question)? — and returns a structured decision plus a short user reply. The
//! router in `agent::run_freeform_text` keys off `intent`, so new intents can be
//! added to the taxonomy without touching the write/chat plumbing.
//!
//! Replaces the old fused tool-calling agent for freeform text: the heavy
//! write itself still runs in `refine::run_write`, exactly as before — this just
//! makes the "what does the user want" decision explicit, consistent, and cheap
//! (one structured call, biased to `content` when unsure so real notes are never
//! dropped).

use serde::Deserialize;

use crate::ai;
use crate::error::Result;
use crate::models::{AiEndpoint, ChatMessage};

/// Structured classifier output. Unknown/extra fields are ignored, missing
/// fields default — so a sloppy model response still parses.
#[derive(Debug, Deserialize)]
pub struct IntentDecision {
    /// social | smalltalk | question | content | edit
    pub intent: String,
    /// append | tidy | restructure — only meaningful when the turn writes.
    #[serde(default)]
    pub write_intent: Option<String>,
    /// Short assistant reply shown to the user (格식체, response_lang).
    #[serde(default)]
    pub reply: String,
}

impl IntentDecision {
    /// Whether this turn writes to the note body.
    pub fn writes(&self) -> bool {
        matches!(self.intent.as_str(), "content" | "edit")
    }

    /// write_note intent, defaulting to append.
    pub fn write_intent(&self) -> &str {
        self.write_intent.as_deref().unwrap_or("append")
    }

    /// Safe fallback when the LLM output can't be parsed — bias to writing so a
    /// real note is never silently dropped.
    fn content_fallback() -> Self {
        IntentDecision {
            intent: "content".to_string(),
            write_intent: Some("append".to_string()),
            reply: String::new(),
        }
    }
}

const CLASSIFY_SYSTEM: &str = r##"당신은 echo 노트 필기형의 '의도 분류기'입니다. 사용자가 채팅으로 보낸 한 메시지를 보고 (1) 노트에 적을 내용인지 (2) 도우미에게 건네는 대화일 뿐인지 판단하고, 사용자에게 보낼 짧은 답변도 함께 만듭니다.

반드시 아래 JSON 객체 **하나만** 출력하세요. 코드펜스·설명·여분 텍스트 금지:
{"intent":"<social|smalltalk|question|content|edit>","write_intent":"<append|tidy|restructure 또는 null>","reply":"<사용자에게 보낼 1~2문장>"}

intent 정의:
- "social" — 도우미에게 건네는 인사·감사·사교적 리액션·작별. 예: '안녕','안뇽','반가워','만나서 반가워','고마워','수고했어','잘 부탁해', 단독 'ㅎㅎ'/'ㅋㅋ'. 노트에 적지 않음.
- "smalltalk" — 노트 주제와 무관한 혼잣말·컨디션·잡담. 예: '배고프다','졸리다','커피 마실까','오늘 피곤하다'. 노트에 적지 않음.
- "question" — 노트 내용이나 진행 상태를 묻는 질문. 예: '방금 뭐 적었어?','결론이 뭐야?'. 노트에 적지 않음. [현재 노트]를 근거로 reply에 답한다.
- "content" — 노트에 담을 실제 내용(사실·생각·메모·키워드·평가·칭찬·아이디어·할 일·사용자가 던지는 의문문 등). 짧은 한 줄도 포함. 노트에 적음.
- "edit" — 이미 적힌 노트를 손보라는 지시. 예: '만나서반가워는 적지마','제목을 X로','표로 바꿔줘','A를 B로 정정','구분선 빼줘'. 노트에 적음(수정).

write_intent (content/edit일 때만, 그 외엔 null):
- "append" — 새 내용을 기존 노트에 이어 적기(기본). 정정·삭제 지시도 append.
- "tidy" — 이미 적힌 내용의 말투·문체만 다듬기(구조·제목은 그대로).
- "restructure" — 소제목·불릿·표 등 구조/스타일을 바꾸라고 **명시**했을 때만.

판단 원칙:
- **발화 전체가** 인사·사교적 리액션·잡담일 때만 social/smalltalk이다. 인사말이 일부로 섞였어도 실제 내용이 있으면 content다. 예: '반가워, 오늘 안건은 결제 연동이야' → content.
- 애매하면 content로 편향한다 — 진짜 노트를 놓치는 것이 사소한 잡담을 적는 것보다 나쁘다. 명백한 인사·잡담·질문만 걸러낸다.
- reply는 짧고 단단하게(격식체). content/edit이면 반영했다는 자연스러운 한 줄 + (필요시) 다음 행동 제안. social/smalltalk이면 가볍게 응대. question이면 [현재 노트]를 근거로 답하되 본문을 통째로 인용하지 말 것.
- **social/smalltalk엔 매번 같은 꼬리말(특히 '편하게 말씀해 주세요')을 반복하지 말 것.** 인사엔 짧고 자연스럽게만 응대하고, 그런 유도 문구는 대화 맨 처음 한 번이면 충분하다. [최근 대화]의 직전 응답과 같은/비슷한 문장을 다시 쓰지 말 것.
- reply에 내부 처리 방식('본문 첫 줄을 제목으로' 등)이나 '제목처럼 보이게 할까요' 같은 군더더기 후속 제안을 넣지 말 것."##;

/// Classify one freeform text turn. Propagates a hard LLM error (caller surfaces
/// it); a merely unparseable response falls back to `content` (write).
pub async fn classify(
    llm: &AiEndpoint,
    note_body: Option<&str>,
    history: &[ChatMessage],
    user_message: &str,
    response_lang: &str,
) -> Result<IntentDecision> {
    let body = note_body
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("(빈 노트)");

    // Last few user/assistant turns for context (e.g. "그건 적지마" 가 무엇을 가리키는지).
    let recent: Vec<String> = history
        .iter()
        .filter(|m| matches!(m.role.as_str(), "user" | "assistant") && !m.content.trim().is_empty())
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|m| format!("{}: {}", m.role, m.content.trim()))
        .collect();
    let recent_block = if recent.is_empty() {
        "(없음)".to_string()
    } else {
        recent.join("\n")
    };

    let user_content = format!(
        "[현재 노트]\n{body}\n\n[최근 대화]\n{recent_block}\n\n[사용자 메시지]\n{}",
        user_message.trim()
    );

    let lang_line = if response_lang == "en" {
        "Write the `reply` field in English."
    } else {
        "reply 필드는 한국어 격식체로 작성한다."
    };
    let system = format!("{CLASSIFY_SYSTEM}\n\n{lang_line}");

    let res = ai::chat_completion(llm, &system, &user_content).await?;
    Ok(parse_decision(&res.content))
}

/// Extract the JSON object from the model output (tolerates stray prose / fences)
/// and deserialize; unparseable → safe content fallback.
fn parse_decision(raw: &str) -> IntentDecision {
    let trimmed = raw.trim();
    let slice = match (trimmed.find('{'), trimmed.rfind('}')) {
        (Some(a), Some(b)) if b > a => &trimmed[a..=b],
        _ => trimmed,
    };
    serde_json::from_str::<IntentDecision>(slice).unwrap_or_else(|_| IntentDecision::content_fallback())
}
