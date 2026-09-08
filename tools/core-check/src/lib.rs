//! Runs the production transport, evidence and persistence code without a GUI.
//! This is complementary to desktop/real-model E2E, not a replacement for it.
#![allow(dead_code)]

pub mod error {
    #[derive(Debug, thiserror::Error)]
    pub enum Error {
        #[error("{0}")]
        Other(String),
        #[error("{0}")]
        InvalidInput(String),
        #[error("{0}")]
        NotFound(String),
        #[error("{0}")]
        Database(#[from] sqlx::Error),
        #[error("{0}")]
        Io(#[from] std::io::Error),
    }
    pub type Result<T> = std::result::Result<T, Error>;
}
pub mod db {
    pub type DbPool = sqlx::SqlitePool;
}
#[path = "../../../src-tauri/src/ai.rs"]
pub mod ai;
#[path = "../../../src-tauri/src/asr.rs"]
pub mod asr;
#[path = "../../../src-tauri/src/chat/edit.rs"]
pub mod edit;
#[path = "../../../src-tauri/src/ffmpeg.rs"]
pub mod ffmpeg;
#[path = "../../../src-tauri/src/models.rs"]
pub mod models;
#[path = "../../../src-tauri/src/repo/note_bodies.rs"]
pub mod note_bodies;
#[path = "../../../src-tauri/src/chat/note_view.rs"]
pub mod note_view;
#[path = "../../../src-tauri/src/chat/prompt.rs"]
pub mod prompt;
#[path = "../../../src-tauri/src/chat/source.rs"]
pub mod source;
#[path = "../../../src-tauri/src/sse.rs"]
pub mod sse;
#[path = "../../../src-tauri/src/storage.rs"]
pub mod storage;
#[path = "../../../src-tauri/src/chat/tools.rs"]
pub mod tools;
#[path = "../../../src-tauri/src/repo/transcripts.rs"]
pub mod transcripts;
pub mod repo {
    pub use crate::{note_bodies, transcripts};
}
#[path = "../../../src-tauri/src/prompts.rs"]
pub mod prompts;
#[cfg(test)]
mod tests;
