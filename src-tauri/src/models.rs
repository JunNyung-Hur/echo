//! Database row models — mapped 1:1 to migration/001_initial.sql.
//!
//! Naming follows D-003 (meeting→note) and D-015 (minutes→note_body).
//! SQLite stores timestamps as ISO-8601 TEXT, UUIDs as TEXT(36), bools as INTEGER 0/1.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// ============================================================================
// notes
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub language: String,
    pub started_at: Option<String>,
    pub source_type: String,
    /// "minutes" | "freeform" — null = 미선택(진입 시 유형 선택). 선택 후 고정.
    pub note_type: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// List-view row — adds `has_active_task` derived from transcripts/note_bodies
/// status (G-LIST-007 / F-LIST-007).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NoteListItem {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub started_at: Option<String>,
    pub note_type: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[sqlx(rename = "has_active_task")]
    #[serde(rename = "has_active_task")]
    pub has_active_task: i64,
    /// Tags on this note — filled by repo::notes::list after the row query
    /// (sqlx-skipped; not a real column).
    #[sqlx(skip)]
    #[serde(default)]
    pub tags: Vec<Tag>,
}

// ============================================================================
// recordings
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Recording {
    pub id: String,
    pub note_id: String,
    pub file_path: String,
    pub original_filename: String,
    pub duration: Option<f64>,
    /// recording / finalizing / webm / mp3 / wav / m4a / failed
    pub format: String,
    /// G-REC-002 heartbeat — updated on each chunk.
    pub last_chunk_at: Option<String>,
    pub finalized_at: Option<String>,
    /// Step 3/4: set when a chat send consumes this attachment. NULL = still a
    /// pending attachment chip (restored on note re-entry).
    pub consumed_at: Option<String>,
    /// Step 4: the user chat message that sent this recording — drives the
    /// attachment chips shown inside that message's bubble.
    pub chat_message_id: Option<String>,
    pub created_at: String,
}

// ============================================================================
// transcripts
// ============================================================================

// Phase 2: constructed by the (not-yet-built) transcribe worker.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Transcript {
    pub id: String,
    pub note_id: String,
    pub recording_id: Option<String>,
    pub raw_path: Option<String>,
    pub corrected_path: Option<String>,
    /// pending / processing / completed / failed / cancelled / empty
    pub status: String,
    pub task_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ============================================================================
// note_bodies  (old: minutes — D-015)
// ============================================================================

// Phase 2: constructed by the (not-yet-built) generate worker.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NoteBody {
    pub id: String,
    pub note_id: String,
    pub transcript_id: Option<String>,
    pub content_path: Option<String>,
    /// pending / processing / completed / failed
    pub status: String,
    pub task_id: Option<String>,
    /// G-DB-001 — NOT NULL + valid JSON.
    pub context_snapshot: String,
    /// G-TASK-007 — stage-1 baseline, captured once, carried across versions.
    pub initial_content_path: Option<String>,
    pub initial_context_snapshot: Option<String>,
    /// G-VERSION-002/003 — archive-and-create + manual edit flag.
    pub archived: i64,
    pub is_manual_edit: i64,
    pub created_at: String,
    pub updated_at: String,
}

// ============================================================================
// note_chat_messages  (old: meeting_chat_messages)
// ============================================================================

// Phase 3: chat agent.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChatMessage {
    pub id: String,
    pub note_id: String,
    /// user | assistant
    pub role: String,
    pub content: String,
    /// G-SSE-005 — "이 시점 노트 보기" chip linkage.
    pub note_body_version_id: Option<String>,
    /// G-SSE-001 — JSON [{id,name,args,result}] for turn-merge persistence.
    pub tool_calls: Option<String>,
    pub created_at: String,
    /// Step 4: recordings this user message sent (filled by chat::list_for_note;
    /// sqlx-skipped — not a real column). Rendered as chips in the bubble.
    #[sqlx(skip)]
    #[serde(default)]
    pub recordings: Vec<Recording>,
}

// ============================================================================
// note_timeline_events  (G-LIFE-001/002 — lifecycle marks)
// ============================================================================

// Phase 3: surfaced as chat pill rows.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TimelineEvent {
    pub id: String,
    pub note_id: String,
    pub kind: String,
    pub content: String,
    pub created_at: String,
}

// ============================================================================
// ai_endpoints  (old: ai_models, slimmed for 1-user app — D-007/D-016)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AiEndpoint {
    pub id: String,
    /// llm | asr
    pub kind: String,
    pub name: String,
    pub model_id: String,
    pub api_base_url: String,
    /// D-016 — stored plaintext in SQLite for v1.
    pub api_key: String,
    /// eb0b667 — "chat_completions" (audio_url-style /chat/completions) |
    /// "transcriptions" (multipart /audio/transcriptions). Was `audio_format`.
    pub request_mode: String,
    pub chunk_seconds: Option<i64>,
    pub max_tokens: Option<i64>,
    pub is_active: i64,
    pub created_at: String,
    pub updated_at: String,
}

// ============================================================================
// tags + note_tags  (Phase 4 — F-TAG / G-TAG)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub created_at: String,
}

/// A tag plus how many notes carry it — drives the filter sidebar (counts +
/// hiding orphans). `usage` is a computed COUNT, not a stored column.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TagWithCount {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub created_at: String,
    pub usage: i64,
}
