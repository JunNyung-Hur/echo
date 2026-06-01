//! ffmpeg system-dependency helpers (D-022).
//!
//! We shell out to `ffmpeg` on PATH instead of bundling — see decisions.md.
//! If absent, callers degrade gracefully (G-REC-011: format='failed' + chunks
//! preserved so user can install ffmpeg and retry).

use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::error::{Error, Result};

/// Resolve a bundled sidecar binary (`<name>.exe` next to the app executable —
/// shipped via tauri.conf `externalBin`). Falls back to bare `name` on PATH for
/// dev runs / installs without the bundled binary.
fn resolve_binary(name: &str) -> PathBuf {
    let file = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            let p = dir.join(&file);
            if p.exists() {
                return p;
            }
        }
    }
    PathBuf::from(name)
}

/// An `ffmpeg` Command that doesn't flash a console window on Windows
/// (CREATE_NO_WINDOW). Without it, each GUI-spawned ffmpeg pops a terminal.
/// Prefers the bundled sidecar (installed build), else `ffmpeg` on PATH (dev).
pub fn command() -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(resolve_binary("ffmpeg"));
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    cmd
}

/// An `ffprobe` Command (sibling binary of ffmpeg) with no console window on
/// Windows — used to read media metadata (duration).
fn ffprobe_command() -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(resolve_binary("ffprobe"));
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    cmd
}

/// Probe a media file's duration in seconds via `ffprobe`. Best-effort
/// (G-REC-010): returns `None` if ffprobe is absent or the probe fails, so
/// callers store `None` and the UI just falls back to 0:00.
pub async fn probe_duration(path: &Path) -> Option<f64> {
    let output = ffprobe_command()
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|d| d.is_finite() && *d > 0.0)
}

/// Is `ffmpeg` resolvable on PATH? Cheap check at app startup.
pub async fn is_available() -> bool {
    command()
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Concat WAV chunks (all same sample-rate/channels) into a single Opus-in-WebM
/// file via ffmpeg's concat demuxer + libopus re-encode.
///
/// WAV chunks each carry their own header so they can't be byte-concatenated;
/// the concat demuxer stitches them at the container level, then we re-encode
/// to Opus (small, good quality, what the transcribe step expects).
pub async fn concat_wavs_to_webm(chunks: &[PathBuf], output: &Path) -> Result<()> {
    if chunks.is_empty() {
        return Err(Error::Other("no chunks to concat".into()));
    }

    // ffmpeg concat demuxer reads a list file: `file '<path>'` per line.
    // Forward slashes are safest cross-platform inside the quoted path.
    let list_path = output.with_extension("concat.txt");
    let mut content = String::new();
    for c in chunks {
        let p = c.to_string_lossy().replace('\\', "/");
        // single-quote the path; escape any embedded single quotes
        let escaped = p.replace('\'', "'\\''");
        content.push_str(&format!("file '{escaped}'\n"));
    }
    tokio::fs::write(&list_path, content)
        .await
        .map_err(|e| Error::Other(format!("concat list write failed: {e}")))?;

    let status = command()
        .args(["-y", "-f", "concat", "-safe", "0", "-i"])
        .arg(&list_path)
        .args(["-c:a", "libopus", "-b:a", "64k"])
        .arg(output)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map_err(|e| Error::Other(format!("ffmpeg spawn failed: {e}")))?;

    let _ = tokio::fs::remove_file(&list_path).await;

    if !status.success() {
        return Err(Error::Other(format!(
            "ffmpeg concat exited with {}",
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}

/// F-REC-004 — convert an arbitrary imported audio (or A/V) file into a single
/// Opus-in-WebM file — the *same shape* cpal-finalize produces — so imported
/// files ride the identical transcribe→generate chain with no special-casing.
/// `-vn` drops any video stream (the user may drop an .mp4/.mkv with audio).
pub async fn convert_to_webm(src: &Path, output: &Path) -> Result<()> {
    if let Some(parent) = output.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| Error::Other(format!("import dir create failed: {e}")))?;
    }
    let status = command()
        .args(["-y", "-i"])
        .arg(src)
        .args(["-vn", "-c:a", "libopus", "-b:a", "64k"])
        .arg(output)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map_err(|e| Error::Other(format!("ffmpeg spawn failed: {e}")))?;
    if !status.success() {
        return Err(Error::Other(format!(
            "ffmpeg convert exited with {} — 지원하지 않는 오디오 형식일 수 있어요",
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}
