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
No echo account or hosted echo backend is required. Metadata is stored in local SQLite; recordings, transcripts, and note bodies are local files. AI processing uses the endpoints you configure.

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
Adding new freeform content preserves existing text. Requests to rewrite, shorten, or reorganize can change it; inspect the edit diff and use *History* to restore a previous version. Stale agent edits are rejected if the note has changed since the agent read it.

### Q. I attached a recording to a freeform note and sent it, but nothing changed.
Make sure both an **ASR** and an **LLM** endpoint are registered in Settings — transcription and organizing need both.

### Q. Can the agent do "just remove the divider" inside the note?
Yes. In-body elements (dividers, bold, tables) and genre switches are content edits, applied in place — expand the edit card in chat to see exactly what changed. Visual design is separate: freeform notes switch **note styles** (ask the agent or use the *Note style* button); minutes notes keep a fixed look.

### Q. I can't find an old note.
Search by title/memo/location keywords, narrow by `#tag`, or scope by date with the *date* chip. `Ctrl+K` focuses the search bar.

### Q. Does retrying repeat all the transcription work?
A failed-task retry reuses successful chunks when the recording and ASR configuration match. A failed chunk prevents the transcript from being marked complete; existing notes are preserved during recovery. Explicit full re-transcription is separate and can replace previous work.

### Q. Can the agent read the original transcript?
Yes. It can search and read completed transcripts attached to the current note, including text beyond the preview. Search uses lexical matching and is not a whole-library semantic index.

### Q. The agent sometimes can't answer.
Check the endpoint connection, quota, tool-calling support, and output token limit. Truncated responses and malformed tool calls are rejected. A missing source or a lexical search miss can also prevent a grounded answer; try a more specific source question.

### Q. Where is my data stored?
Metadata is stored in SQLite in your app-data folder (`…/com.echo.app/echo.db`); recordings, transcripts, and bodies are files in that folder. Audio and text needed for AI processing are sent to your configured endpoints. Choose local endpoints if you want that processing to stay on your machine. API keys are stored locally in plaintext in this version.

---

## 3. Developers

### Q. How do I run it locally?
From the repo root:
```bash
npm ci                      # root: Tauri CLI
npm ci --prefix src-ui      # frontend deps
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
On send, each attached recording is transcribed, then the conversation editor reads its original evidence and adds or edits the note. Adding new content preserves the existing text in code; reorganizing existing content uses explicit edits and version history. Failed attachments are reported separately.

---

## 4. Troubleshooting checklist

- App won't start: check `npm run dev` logs (vite / cargo errors).
- No recording: input source & test under Settings → Audio, OS input permission.
- Transcription never finishes: is the registered ASR endpoint responding? Network?
- Weak note result: ask the agent ("shorter", "bold the decisions", "as lecture notes", "it's X', not X").
- Agent can't answer: LLM endpoint outage/quota possible — check the red notice and resend.
