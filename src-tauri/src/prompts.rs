//! Korean LLM prompts ported verbatim from the old worker (Phase 2).
//!
//! Kept as `include_str!` of sibling `.md` files so the tuned text isn't
//! mangled by Rust string escaping. These are faithful ports — do NOT
//! paraphrase or reflow; the wording is load-bearing (D-013).

#![allow(dead_code)] // Phase 2: consumed by transcribe (normalizer) + generate.

/// Stage-1 minutes generation (개조식 **마크다운**, content-proportional sizing).
/// Source: worker/app/prompts/minutes.py `MINUTES_SYSTEM_PROMPT` @ Meetzy d75150c
/// (마크다운 전환 ab0ba14 + 제목 rule 6 + ASR 표기 통일 #7). 본문엔 `__RULE2__`
/// 자리표시자만 두고 언어별 rule 2를 `minutes_system_prompt`가 끼운다.
/// 도메인 워딩만 회의록→노트로 옮긴 것 외에는 Meetzy 원문 그대로.
const MINUTES_SYSTEM_PROMPT_TEMPLATE: &str = include_str!("prompts/minutes_system.md");

// rule 2(출력 언어)의 단일 출처. KO 문구는 `.md`의 원래 rule 2와 글자 단위로
// 일치해야 한다(자리표시자 치환 시 byte-identical 보장).
const MINUTES_RULE2_KO: &str =
    "2. **Language** — Write the entire note in Korean. Translate other languages faithfully, retaining proper nouns and technical terms.";
const MINUTES_RULE2_EN: &str = "2. **Language** — Write the entire minutes in ENGLISH. If the transcript is in another language (e.g. Korean), translate the content into natural English while preserving proper nouns, numbers, dates, and product/technical terms faithfully (transliterate personal/company names; keep the original-language term in parentheses when there is no clean equivalent). Non-negotiable.";

/// 노트 본문 시스템 프롬프트. `target_lang="en"`이면 rule 2를 영어 번역 규칙으로 치환
/// + 최상단에 강한 영어 지시(약한 모델이 한국어 전사에 끌리는 것 차단). 그 외(ko
/// 포함)는 KO rule 2만 끼운 byte-identical 프롬프트.
pub fn minutes_system_prompt(target_lang: &str) -> String {
    if target_lang == "en" {
        let body = MINUTES_SYSTEM_PROMPT_TEMPLATE.replace("__RULE2__", MINUTES_RULE2_EN);
        format!(
            "# OUTPUT LANGUAGE — ENGLISH ONLY\nWrite the ENTIRE minutes in English. The transcript is likely in Korean — translate its content into natural English. Do NOT output Korean sentences.\n\n{body}"
        )
    } else {
        MINUTES_SYSTEM_PROMPT_TEMPLATE.replace("__RULE2__", MINUTES_RULE2_KO)
    }
}

/// One-line list-preview summary (fills note.description once).
/// Source: worker/app/prompts/minutes.py `MINUTES_ONE_LINE_SUMMARY_PROMPT`.
pub const MINUTES_ONE_LINE_SUMMARY_PROMPT: &str =
    include_str!("prompts/minutes_one_line_summary.md");

// Stage-2 refine 프롬프트는 2차 싱크에서 폐기 — 편집은 chat/exec.rs 의
// edit_minutes(str_replace)가 담당한다 (Meetzy 992249c·6d3d07c).
