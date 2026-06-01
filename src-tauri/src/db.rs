use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::path::Path;

use crate::error::Result;

pub type DbPool = SqlitePool;

/// Initialize the SQLite connection pool with all required PRAGMAs and run
/// migrations. Called once at app startup.
///
/// invariant: G-DB-003 — `PRAGMA foreign_keys = ON` enforced on every
/// connection via SqliteConnectOptions::foreign_keys(true).
pub async fn init_pool(db_path: &Path) -> Result<DbPool> {
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;

    // Run all migrations in migrations/. Idempotent.
    sqlx::migrate!("./migrations").run(&pool).await?;

    tracing::info!("database pool initialized and migrations applied");
    Ok(pool)
}
