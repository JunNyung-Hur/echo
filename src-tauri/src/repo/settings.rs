//! settings repository — simple KV store (single-user app).
//!
//! The `settings` table is `(key TEXT PRIMARY KEY, value TEXT)`. Used for app-
//! wide preferences that don't warrant their own table — currently the UI
//! language (`ui_lang`). `get` returns None when the key was never set so
//! callers can fall back to a default.

use sqlx::SqlitePool;

use crate::error::Result;

/// Read a setting's value, or None if the key was never set.
pub async fn get(pool: &SqlitePool, key: &str) -> Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(v,)| v))
}

/// Upsert a setting (insert or overwrite the existing value).
pub async fn set(pool: &SqlitePool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}
