//! Chat agent — Meetzy d75150c `chat_agent/` 이식 (2차 싱크).
//!
//!   tools     — tool specs (read/edit_minutes, set_theme, ask_user, …) + gating
//!   edit      — str_replace 편집 엔진 (apply_str_edits + 가드)
//!   exec      — tool dispatch handlers (talker=doer)
//!   prompt    — system-prompt builder (일반화 프롬프트 + 정직·턴 규칙)
//!   agent     — 단일 세션 루프 + parts 누적 + ask_user 하드스톱 (freeform 포함)
//!   refine    — freeform write / 첨부 map-reduce (echo 고유)
//!
//! 한국어 프롬프트/도구 설명 워딩은 Meetzy 원문 기준 — 임의 paraphrase 금지.

pub mod agent;
pub mod work_context;
pub mod edit;
pub mod exec;
pub mod prompt;
pub mod refine;
pub mod tools;
pub mod source;
pub mod note_view;
