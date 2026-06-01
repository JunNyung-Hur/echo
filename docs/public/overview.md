# echo — Overview

Anything that crosses your mind — a meeting, a lecture, an interview, a memo, thinking out loud — record it (or jot it), and an AI agent beside your note transcribes it and organizes it into clean notes. Ask in plain language and it refines them. echo is a personal **Second Brain desktop app**.

---

## 1. In one line

From capture to organized notes, everything happens on your laptop, and the organizing is done by asking an AI agent like a colleague sitting next to you. No account, no server — all local.

---

## 2. Why it exists

- Recording, transcribing, and organizing are usually scattered across separate apps you have to stitch together by hand — and even the organizing step means hunting through menus and buttons.
- Meeting-minutes tools assume a "meeting," but what you actually want to keep is much broader — lectures, interviews, memos, brainstorming.
- echo brings this into one desktop app, lets you organize a note by asking the agent next to it in a single line, and shapes the result to whatever the input is.

---

## 3. What makes it different

- **Local-first, personal** — no account or server; everything lives in local SQLite on your device. Built slim, for one user.
- **An AI agent beside the note** — fix details, restructure, summarize, change the style, recover from failures, all in natural language, no menu hunting. The agent knows where the note stands and suggests the next step.
- **Two kinds of notes** — *Minutes* (record → transcribe → structured write-up) and *Freeform* (a chat-first notepad you grow by typing and by attaching voice/audio that echo transcribes and weaves in).
- **Organizing that adapts to the input** — minutes for a meeting, organized information for a lecture/briefing, a tight summary for a short monologue.
- **You stay in control of the result** — edit a generated note by hand or refine it in plain language, and roll back to any earlier version.
- **Bring your own models** — connect any OpenAI-compatible LLM and ASR endpoint, cloud or local (Ollama).

---

## 4. Key features

### Capture

- Microphone or system-sound input (native capture).
- Chunked processing to minimize data loss across interruptions.
- Import existing audio files (mp3, wav, m4a, webm, …).
- Freeform notes: attach one or more recordings/files right in the chat, before sending.

### Transcription & note organizing

- ASR results are post-processed by an LLM for formatting and mis-hearing fixes.
- For freeform attachments, each recording is transcribed and merged into the note (existing content preserved, different topics split into sections).
- Length and structure adjust to the amount and nature of the input.

### The AI agent

- Restructure, summarize, change wording, switch the note's genre/design.
- Correct mistaken words in place ("it's X', not X").
- Retry a failed transcription or generation.
- A one-line next-step suggestion after each action; progress shown in the chat for longer tasks.

### Tags & search

- Hashtags (`#tag`) with autocomplete, shown in the order added.
- Search by text or `#tag` (tags AND-matched), with a date filter.

### Note management

- Expand version history and roll back to any point.
- Edit a note by hand.
- Manage the note's recordings (replay / delete) in its archive.

### Bilingual

- Full Korean / English UI, plus a selectable AI output language.

---

## 5. Tech stack

| Area | Tech |
|---|---|
| App shell | Tauri v2 (native desktop) |
| Frontend | React, TypeScript, Vite, Tailwind |
| Backend | Rust (sqlx, tokio) |
| Storage | Local SQLite |
| Audio | cpal (native capture), bundled ffmpeg |
| AI | OpenAI-compatible LLM · ASR endpoints (bring-your-own) |

---

## 6. Lineage

echo grew out of **Meetzy**, a web-based meeting-minutes app, rebuilt from scratch as a personal desktop Second Brain.

---

## 7. Where to next

- [User guide](user-guide.md) — flows and walkthrough
- [Architecture](architecture.md) — system structure & runtime
- [FAQ](faq.md)
- [Requirements](requirements.md)
- [Release notes](release-notes/)
