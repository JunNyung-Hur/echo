# echo

**A personal Second Brain that lives on your laptop.**

echo is a desktop app for capturing anything you think or hear — meetings, lectures, interviews, quick memos, talking to yourself — and turning it into clean notes. Record (or drop in an audio file), and an AI agent sitting next to your note transcribes it and weaves it into the note for you. Ask the agent in plain language and it refines the note. It's a single-user app — no account, no server.

![echo — note list](docs/public/screenshots/note_list.png)

---

## Highlights

- **Two kinds of notes, shaped to whatever you capture**
  - **Minutes** — record a meeting/lecture → echo transcribes it and generates a structured write-up (decisions, action items, sections), sized to the content.
  - **Freeform** — a chat-first notepad. Jot text, or attach voice recordings / audio files right in the chat; echo transcribes each and merges them into your note, keeping what's already there.
- **An AI agent beside the note** — fix the title, restructure, summarize, change the style, recover from a failed transcription — all by asking in plain language, no menu hunting. The agent knows where the note stands and suggests the next step.
- **Bring your own models** — connect any OpenAI-compatible LLM and ASR endpoint, cloud (OpenAI, etc.) or local (vLLM, …). echo ships no AI of its own.
- **And more** — version history with rollback, `#tag` + text search, and a full Korean / English UI.

---

## Screenshots

**Start a note — pick a type:**

![note type selection](docs/public/screenshots/type_selection.png)

**Freeform — chat-first notes you grow over time:**

| | | |
|:-:|:-:|:-:|
| ![freeform note](docs/public/screenshots/note_example.png) | ![freeform note](docs/public/screenshots/note_example2.png) | ![freeform note](docs/public/screenshots/note_example3.png) |

**Minutes — record a meeting, get a structured write-up:**

| Before | Recording | Transcribing | Done |
|:-:|:-:|:-:|:-:|
| ![before recording](docs/public/screenshots/meeting_before.png) | ![recording](docs/public/screenshots/meeting_recording.png) | ![transcribing](docs/public/screenshots/meeting_transcribing.png) | ![done](docs/public/screenshots/meeting_done.png) |

**Settings — bring your own models, pick your input source:**

| AI models & language | Input source test |
|:-:|:-:|
| ![settings](docs/public/screenshots/settings_example.png) | ![input source test](docs/public/screenshots/input_source_test.png) |

---

## Install (Windows x64)

1. Download the latest **`echo_<version>_x64-setup.exe`** (NSIS installer) from the releases.
2. Run it and follow the prompts.

That's it for the app itself:

- **ffmpeg is bundled** — echo ships ffmpeg/ffprobe alongside the app, so you don't need to install them separately.
- **WebView2** is installed automatically by the installer if missing (pre-installed on Windows 11).

**One setup step inside the app:** echo uses *your* AI models, so open **Settings → AI models** and register an OpenAI-compatible **LLM** endpoint (for note generation/refinement) and an **ASR** endpoint (for transcription). Cloud API keys or a local server both work. Until these are set, recording/transcription and note generation can't run.

> Architecture note: the installer is x64. It runs on Windows 11 ARM via x64 emulation, but ARM is not a build target.

See the **[user guide](docs/public/user-guide.md)** for a full walkthrough.

---

## Build from source

Prerequisites: **Rust** 1.80+, **Node.js** 20+, and the Tauri Windows toolchain (MSVC build tools + Windows 11 SDK).

```bash
npm install                  # root: Tauri CLI
npm --prefix src-ui install  # frontend deps

npm run dev                  # tauri dev (vite + cargo + the app)
```

For development you do **not** need the bundled ffmpeg binaries — the app falls back to `ffmpeg` on your PATH. Install ffmpeg (with ffprobe) for recording/transcription to work in dev.

### Building the release installer

Release builds bundle ffmpeg via an overlay config so the base config stays buildable without the large binaries. Drop an **LGPL** ffmpeg build into `src-tauri/binaries/` (see `src-tauri/binaries/README.md`), then:

```bash
npx tauri build --config src-tauri/tauri.release.conf.json
```

This produces an NSIS `.exe` and an MSI under `src-tauri/target/release/bundle/`.

---

## How it works

echo is a single Tauri v2 desktop app: a React webview (`src-ui/`) talks over IPC to a Rust core (`src-tauri/`). Notes live in local SQLite; heavy work (audio finalize, transcription, note generation) runs on async Tokio workers; AI calls go to your registered OpenAI-compatible endpoints.

```
src-ui/        React + TypeScript + Vite + Tailwind frontend
src-tauri/     Rust core — Tauri commands, sqlx repos, chat agent, workers
  src/
  migrations/  SQLite schema (applied on startup)
  binaries/    bundled ffmpeg/ffprobe for release builds (gitignored)
docs/public/   user-facing docs (overview, user guide, architecture, FAQ)
```

Data lives in your OS app-data dir: `%APPDATA%\com.echo.app\echo.db` on Windows (and the equivalent on macOS/Linux). Deleting a note removes its recordings, transcripts, and bodies on disk too.

More detail in **[docs/public/architecture.md](docs/public/architecture.md)**.


## License

echo is licensed under the **Apache License 2.0** — see [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).

## Third-party

echo bundles an **LGPL** build of [FFmpeg](https://ffmpeg.org) (audio only — no GPL codecs), invoked as a separate process, so it doesn't affect echo's own license. FFmpeg is a trademark of Fabrice Bellard. See [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md) for attribution, source, and license terms.
