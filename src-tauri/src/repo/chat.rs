//! note_chat_messages repository (Phase 3 chat agent).

#![allow(dead_code)] // Phase 3: wired by the agent + chat commands.

use std::collections::HashMap;

use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use uuid::Uuid;

use crate::error::Result;
use crate::models::{ChatMessage, Recording};

pub async fn list_for_note(pool: &SqlitePool, note_id: &str) -> Result<Vec<ChatMessage>> {
    let mut msgs = sqlx::query_as::<_, ChatMessage>(
        "SELECT * FROM note_chat_messages WHERE note_id = ? ORDER BY created_at ASC, rowid ASC",
    )
    .bind(note_id)
    .fetch_all(pool)
    .await?;
    attach_recordings(pool, &mut msgs).await?;
    Ok(msgs)
}

/// Fill each user message's `recordings` (Step 4 attachment chips) in one query.
async fn attach_recordings(pool: &SqlitePool, msgs: &mut [ChatMessage]) -> Result<()> {
    let ids: Vec<&str> = msgs
        .iter()
        .filter(|m| m.role == "user")
        .map(|m| m.id.as_str())
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    let mut qb: QueryBuilder<Sqlite> =
        QueryBuilder::new("SELECT * FROM recordings WHERE chat_message_id IN (");
    let mut sep = qb.separated(", ");
    for id in &ids {
        sep.push_bind(*id);
    }
    qb.push(") ORDER BY created_at ASC");

    let recs: Vec<Recording> = qb.build_query_as::<Recording>().fetch_all(pool).await?;
    let mut by_msg: HashMap<String, Vec<Recording>> = HashMap::new();
    for r in recs {
        if let Some(mid) = r.chat_message_id.clone() {
            by_msg.entry(mid).or_default().push(r);
        }
    }
    for m in msgs.iter_mut() {
        if let Some(rs) = by_msg.remove(&m.id) {
            m.recordings = rs;
        }
    }
    Ok(())
}

/// Persist one chat row. `tool_calls_json` = JSON array of {id,name,args,result}
/// for an assistant turn that called tools (G-SSE-001), else None.
pub async fn create(
    pool: &SqlitePool,
    note_id: &str,
    role: &str,
    content: &str,
    tool_calls_json: Option<&str>,
    body_version_id: Option<&str>,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO note_chat_messages (id, note_id, role, content, tool_calls, note_body_version_id) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(note_id)
    .bind(role)
    .bind(content)
    .bind(tool_calls_json)
    .bind(body_version_id)
    .execute(pool)
    .await?;
    Ok(id)
}
