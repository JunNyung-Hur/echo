//! Note-centric on-disk layout for artifacts (recordings, transcripts, bodies).
//!
//! Everything a note owns lives under `<app_data>/notes/note-<id8>/`. The folder
//! name is derived purely from the note id (NOT the title) — stable, short, and
//! immune to title churn, so there's no folder rename and no DB bookkeeping.
//! Inside, artifacts are flat files named by their own id:
//!   recordings/<rec_id>.webm        (chunks live in <rec_id>.chunks/ during capture)
//!   transcripts/<tid>.txt           (wav chunks in <tid>.chunks/ during transcribe)
//!   bodies/<bid>.html
//!
//! Paths are stored in the DB **relative to app_data** ("notes/note-…/…") and
//! resolved to an absolute path via [`resolve`] at every read — keeping the data
//! dir portable.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// app_data_dir, captured once at startup (lib.rs `setup`) before any path is
/// built. `OnceLock` lets the repo layer resolve paths without an `AppHandle`.
static APP_DATA: OnceLock<PathBuf> = OnceLock::new();

pub fn init(dir: PathBuf) {
    let _ = APP_DATA.set(dir);
}

pub fn app_data() -> PathBuf {
    APP_DATA.get().cloned().unwrap_or_default()
}

/// Absolute path for a DB-stored path. New paths are app_data-relative
/// ("notes/note-…/…"); an already-absolute path (legacy/edge) passes through.
pub fn resolve(stored: &str) -> PathBuf {
    let p = Path::new(stored);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        app_data().join(stored)
    }
}

/// Stable, id-derived folder name for a note (`note-<first 8 of id>`).
pub fn note_dir_name(note_id: &str) -> String {
    format!("note-{}", note_id.chars().take(8).collect::<String>())
}

/// app_data-relative note dir ("notes/note-<id8>").
pub fn note_rel_dir(note_id: &str) -> String {
    format!("notes/{}", note_dir_name(note_id))
}

pub fn note_abs_dir(note_id: &str) -> PathBuf {
    app_data().join("notes").join(note_dir_name(note_id))
}

// ---- flat artifact paths (app_data-relative; the strings stored in the DB) ----

pub fn recording_webm_rel(note_id: &str, recording_id: &str) -> String {
    format!("{}/recordings/{}.webm", note_rel_dir(note_id), recording_id)
}

/// Transient WAV chunks during capture; removed by finalize.
pub fn recording_chunks_rel(note_id: &str, recording_id: &str) -> String {
    format!("{}/recordings/{}.chunks", note_rel_dir(note_id), recording_id)
}

pub fn transcript_text_rel(note_id: &str, transcript_id: &str) -> String {
    format!("{}/transcripts/{}.txt", note_rel_dir(note_id), transcript_id)
}

/// Transient WAV chunks during transcription; removed when the text is written.
pub fn transcript_chunks_rel(note_id: &str, transcript_id: &str) -> String {
    format!("{}/transcripts/{}.chunks", note_rel_dir(note_id), transcript_id)
}

pub fn body_rel(note_id: &str, body_id: &str) -> String {
    format!("{}/bodies/{}.html", note_rel_dir(note_id), body_id)
}
