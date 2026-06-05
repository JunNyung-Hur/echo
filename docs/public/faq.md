# echo — FAQ

Common questions from both user and developer angles. echo is a personal desktop app that runs on your laptop with no account or server.

---

## 1. General

### Q. What does echo do?
Capture anything — by recording it or jotting it — and AI transcribes and organizes it into a note. You refine the note by asking an AI agent beside it in plain language.

### Q. What is it good for?
- Meetings with clear decisions/actions (minutes).
- Information-heavy briefings, lectures, seminars (organized notes).
- Short monologues, memos, brainstorming (tight summaries).

The note's shape and length adjust to the input.

### Q. What are the two note types?
- **Minutes** — record a meeting/lecture; echo transcribes it and generates a structured write-up.
- **Freeform** — a chat-first notepad you grow by typing and by attaching voice/audio that echo transcribes and weaves into the note.

### Q. Do I need an account or server?
No. It's single-user with no login, and all data is stored in local SQLite on your device.

### Q. Is AI built in?
No. You connect OpenAI-compatible endpoints yourself — a cloud API key or a local server like vLLM.

### Q. Do I need to install ffmpeg?
No. The installer **bundles ffmpeg** for you. (For a from-source dev build, ffmpeg on your PATH is used instead.)

---

## 2. Users

### Q. I started recording but the waveform isn't moving.
Check and test the input source under Settings → Audio, and verify your OS input device and permission.

### Q. System-sound mode but nothing records.
Confirm system output is the capture target using the test (waveform & level) in Settings.

### Q. My note is too short or too long.
Ask the agent: "shorter", "more detail", "cut to 10 lines", "drop the small talk".

### Q. A transcribed word is wrong.
Tell the agent briefly and the note body is corrected — e.g., "it's Sungkyunkwan, not Seongyeonggwan".

### Q. Do my hand edits survive later refinements?
Yes. Hand edits are preserved through later refinements, and you can roll back to any point from *History*.

### Q. I attached a recording to a freeform note and sent it, but nothing changed.
Make sure both an **ASR** and an **LLM** endpoint are registered in Settings — transcription and organizing need both.

### Q. Can the agent do "just remove the divider" inside the note?
Yes. The agent handles in-body visual elements (dividers, bold, tables) and even genre switches as refinements.

### Q. I can't find an old note.
Search by title/memo/location keywords, narrow by `#tag`, or scope by date with the *date* chip. `Ctrl+K` focuses the search bar.

### Q. The agent sometimes can't answer.
Usually a transient LLM-server issue. A red notice card appears in the chat and clears when you send the next message, which retries.

### Q. Where is my data stored?
Local SQLite in your app-data folder (`…/com.echo.app/echo.db`). The only thing leaving your machine is the calls to the AI endpoints you registered — point those at a local model (e.g. vLLM) and nothing leaves at all.

---

## 3. Developers

### Q. How do I run it locally?
From the repo root:
```bash
npm install                  # root: Tauri CLI
npm --prefix src-ui install  # frontend deps
npm run dev                  # tauri dev (vite + cargo + app)
```
For dev, ffmpeg on your PATH is used (the bundled binaries are release-only).

### Q. Project structure?
| Path | Role |
|---|---|
| `src-ui/` | React + TypeScript + Vite frontend |
| `src-tauri/` | Rust backend (Tauri v2, sqlx, tokio) |
| `src-tauri/migrations/` | SQLite migrations |
| `src-tauri/binaries/` | bundled ffmpeg/ffprobe for release builds (gitignored) |

### Q. How do I build the release installer?
Release builds bundle ffmpeg via an overlay config (so the base config stays buildable without the large binaries). Place an LGPL ffmpeg build into `src-tauri/binaries/` (see its README), then:
```bash
npx tauri build --config src-tauri/tauri.release.conf.json
```

### Q. Where do I change models / DB?
| Change | Where |
|---|---|
| DB schema | `src-tauri/migrations/` (sqlx) |
| Row models | `src-tauri/src/models.rs` |
| AI endpoint config | `ai_endpoints` table (managed in Settings) |

### Q. Where is the chat agent?
Under `src-tauri/src/chat/`: the system-prompt builder (`prompt.rs`), tool specs (`tools.rs`), agent loop (`agent.rs`), and tool execution (`exec.rs`). A snapshot of the user's screen state (`user_state`) is included on each request and feeds both the tool gate and the prompt.

### Q. How are freeform attachments turned into notes?
On send, each attached recording is transcribed, then a map-reduce step drafts each transcript and merges the drafts with the existing note into one — the existing note is one of the merge inputs, so its content is preserved.

---

## 4. Troubleshooting checklist

- App won't start: check `npm run dev` logs (vite / cargo errors).
- No recording: input source & test under Settings → Audio, OS input permission.
- Transcription never finishes: is the registered ASR endpoint responding? Network?
- Weak note result: ask the agent ("shorter", "bold the decisions", "as lecture notes", "it's X', not X").
- Agent can't answer: LLM endpoint outage/quota possible — check the red notice and resend.
