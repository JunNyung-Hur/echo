//! ai_endpoints repository (D-007 / D-016).
//!
//! 1-user slim version of old `ai_models`. No is_system protection flag, no
//! per-user scoping. `is_active` is enforced one-per-kind by a partial unique
//! index (G-DB-006) — `activate` clears siblings in the same transaction.

use serde::Deserialize;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::models::AiEndpoint;

#[derive(Debug, Deserialize)]
pub struct CreateEndpointInput {
    pub kind: String, // llm | asr
    pub name: String,
    pub model_id: String,
    pub api_base_url: String,
    pub api_key: Option<String>,
    pub request_mode: Option<String>,
    pub chunk_seconds: Option<i64>,
    pub max_tokens: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEndpointInput {
    pub name: Option<String>,
    pub model_id: Option<String>,
    pub api_base_url: Option<String>,
    pub api_key: Option<String>,
    pub request_mode: Option<String>,
    pub chunk_seconds: Option<Option<i64>>,
    pub max_tokens: Option<Option<i64>>,
}

fn validate_kind(kind: &str) -> Result<()> {
    if matches!(kind, "llm" | "asr") {
        Ok(())
    } else {
        Err(Error::InvalidInput(format!("kind: {kind}")))
    }
}

pub async fn list(pool: &SqlitePool, kind: Option<&str>) -> Result<Vec<AiEndpoint>> {
    let rows = match kind {
        Some(k) => {
            validate_kind(k)?;
            sqlx::query_as::<_, AiEndpoint>(
                "SELECT * FROM ai_endpoints WHERE kind = ? ORDER BY created_at ASC",
            )
            .bind(k)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, AiEndpoint>(
                "SELECT * FROM ai_endpoints ORDER BY kind, created_at ASC",
            )
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows)
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<AiEndpoint> {
    sqlx::query_as::<_, AiEndpoint>("SELECT * FROM ai_endpoints WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("ai_endpoint {id}")))
}

/// Currently active endpoint for a kind, if any. Phase 2 transcribe/generate
/// resolve their endpoint through this.
#[allow(dead_code)] // Phase 2
pub async fn get_active(pool: &SqlitePool, kind: &str) -> Result<Option<AiEndpoint>> {
    validate_kind(kind)?;
    Ok(sqlx::query_as::<_, AiEndpoint>(
        "SELECT * FROM ai_endpoints WHERE kind = ? AND is_active = 1 LIMIT 1",
    )
    .bind(kind)
    .fetch_optional(pool)
    .await?)
}

pub async fn create(pool: &SqlitePool, input: CreateEndpointInput) -> Result<AiEndpoint> {
    validate_kind(&input.kind)?;
    // Auto-activate the first endpoint of a kind so there's always a default
    // selection (G-DB-006 holds — it's the only one of its kind).
    let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_endpoints WHERE kind = ?")
        .bind(&input.kind)
        .fetch_one(pool)
        .await?;
    let is_active: i64 = if existing == 0 { 1 } else { 0 };
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO ai_endpoints \
         (id, kind, name, model_id, api_base_url, api_key, request_mode, chunk_seconds, max_tokens, is_active) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&input.kind)
    .bind(&input.name)
    .bind(&input.model_id)
    .bind(&input.api_base_url)
    .bind(input.api_key.unwrap_or_default())
    .bind(input.request_mode.unwrap_or_else(|| "chat_completions".into()))
    .bind(input.chunk_seconds)
    .bind(input.max_tokens)
    .bind(is_active)
    .execute(pool)
    .await?;
    get(pool, &id).await
}

pub async fn update(pool: &SqlitePool, id: &str, input: UpdateEndpointInput) -> Result<AiEndpoint> {
    let _existing = get(pool, id).await?;
    // Simpler than a dynamic builder: COALESCE each column.
    sqlx::query(
        "UPDATE ai_endpoints SET \
            name = COALESCE(?, name), \
            model_id = COALESCE(?, model_id), \
            api_base_url = COALESCE(?, api_base_url), \
            api_key = COALESCE(?, api_key), \
            request_mode = COALESCE(?, request_mode), \
            chunk_seconds = ?, \
            max_tokens = ? \
         WHERE id = ?",
    )
    .bind(&input.name)
    .bind(&input.model_id)
    .bind(&input.api_base_url)
    .bind(&input.api_key)
    .bind(&input.request_mode)
    // chunk_seconds / max_tokens: Option<Option<i64>>. Outer None = leave as-is;
    // but COALESCE can't express "leave as-is" with a plain bind, so for these
    // two we read existing when outer None.
    .bind(match input.chunk_seconds {
        Some(v) => v,
        None => _existing.chunk_seconds,
    })
    .bind(match input.max_tokens {
        Some(v) => v,
        None => _existing.max_tokens,
    })
    .bind(id)
    .execute(pool)
    .await?;
    get(pool, id).await
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<()> {
    let target = get(pool, id).await?; // 404 if missing
    sqlx::query("DELETE FROM ai_endpoints WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    // Keep a default selected: if the active one was removed but siblings of
    // the same kind remain, promote the most recent.
    if target.is_active == 1 {
        let next: Option<String> = sqlx::query_scalar(
            "SELECT id FROM ai_endpoints WHERE kind = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(&target.kind)
        .fetch_optional(pool)
        .await?;
        if let Some(next_id) = next {
            sqlx::query("UPDATE ai_endpoints SET is_active = 1 WHERE id = ?")
                .bind(&next_id)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

/// Ensure each kind with ≥1 endpoint has one active (activates the most recent
/// if none is). Run at startup so existing data / imports always have a default
/// selection — covers endpoints registered before auto-activate existed.
pub async fn ensure_default_active(pool: &SqlitePool) -> Result<()> {
    for kind in ["llm", "asr"] {
        let active: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ai_endpoints WHERE kind = ? AND is_active = 1",
        )
        .bind(kind)
        .fetch_one(pool)
        .await?;
        if active > 0 {
            continue;
        }
        let next: Option<String> = sqlx::query_scalar(
            "SELECT id FROM ai_endpoints WHERE kind = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(kind)
        .fetch_optional(pool)
        .await?;
        if let Some(id) = next {
            sqlx::query("UPDATE ai_endpoints SET is_active = 1 WHERE id = ?")
                .bind(&id)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

/// Activate one endpoint for its kind, clearing any sibling.
///
/// invariant: G-DB-006 — one active per kind. We clear-then-set inside a
/// single transaction so the partial unique index never sees two active rows.
pub async fn activate(pool: &SqlitePool, id: &str) -> Result<AiEndpoint> {
    let target = get(pool, id).await?;
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE ai_endpoints SET is_active = 0 WHERE kind = ? AND is_active = 1")
        .bind(&target.kind)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE ai_endpoints SET is_active = 1 WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    get(pool, id).await
}
