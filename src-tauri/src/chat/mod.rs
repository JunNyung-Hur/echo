//! Chat agent (Phase 3) — 1:1 port of backend/app/services/chat_agent/.
//!
//!   tools     — 6 OpenAI tool specs + stage/role/capability gating
//!   prompt    — 14-section system-prompt builder (next)
//!   agent     — streaming agent loop + tool dispatch → Tauri chat_event (next)
//!
//! The old Korean system prompt + tool descriptions are preserved verbatim;
//! the LLM's tuned tool-selection behavior depends on the exact wording and
//! Phase 3 oracle parity is checked against the old stack.

pub mod agent;
pub mod exec;
pub mod prompt;
pub mod refine;
pub mod tools;
