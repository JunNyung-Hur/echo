# echo — Requirements

What echo must provide (the WHAT), by area. The HOW lives in [`architecture.md`](architecture.md). echo is a single-user desktop app with no account or server.

---

## 1. Scope

- Core flows: first-run setup → create a note (minutes or freeform) → capture (record / import / chat-attach) → transcribe → organize → refine with the chat agent.
- Supporting: tags & search, version history, settings (AI endpoints, language, audio).
- External: OpenAI-compatible LLM·ASR endpoints.

Out of scope: mobile apps, real-time multi-user collaboration, calendar integration, accounts/multi-user, full-text body search (later).

---

## 2. Requirements by area

### 2.1 First-run setup

- [MVP] On first launch: ① pick language (KO/EN) → ② register AI endpoints (ASR · LLM).
- [MVP] The chosen language is the default for both UI and AI output, changeable in Settings.
- [MVP] ffmpeg is bundled in the installer; the app must not require the user to install it.

### 2.2 Note creation & types

- [MVP] Creating a note first asks for a **type**: **minutes** or **freeform**. The type is fixed once chosen.
- [MVP] The note list shows a per-type icon in front of each title.
- [MVP] The note **title is derived from the body's first line** (read-only meta — no separate title/date/location editing in the header).
- [MVP] An empty title falls back to a default ("제목 없음" / "Untitled").
- [MVP] A note can be deleted; its recordings, transcripts, and bodies are removed from DB (CASCADE) and disk.

### 2.3 Note list

- [MVP] Chronological agenda grouped by date, with a time-appropriate greeting + today's date.
- [MVP] Case-insensitive search over title/memo/location/tags (server-side, ≥2 chars, debounced); `Ctrl+K` focuses the search bar.
- [MVP] Date-range filter and pagination handled server-side (no N+1); `has_active_task` flags in-progress notes.
- [MVP] An empty state only when there are zero notes; "no results" for empty search/filter.

### 2.4 Freeform notes

- [MVP] A freeform note is chat-first: the agent's `write_note` writes/refines the body; the user can also edit the body directly.
- [MVP] In the chat input the user can attach audio before sending: **record**, **pick a file**, or **drag-and-drop** — multiple attachments per send.
- [MVP] Attachments show as chips (length, play, remove-with-confirm); recording/upload in progress is reflected.
- [MVP] Sending transcribes each attachment and incorporates it into the note via map-reduce — existing content preserved, different topics split into sections.
- [MVP] Sent recordings move to the note's **archive** (header badge with count; replay/delete).
- [MVP] Recordings attached but not yet sent are restored as chips when the note is reopened (no loss if the app closes first).
- [MVP] Only one *in-progress* capture per note at a time; finished recordings may accumulate.

### 2.5 Minutes notes — record / import

- [MVP] cpal-based native recording. Microphone / system-sound sources.
- [MVP] Input source pick & test (waveform · level), OS input-volume control.
- [MVP] Chunked capture to minimize loss on interruption; orphaned recordings auto-recovered on restart.
- [MVP] Import audio files (mp3·wav·m4a·webm…); importing auto-starts transcription.

### 2.6 Transcription

- [MVP] Automatic transcription after recording/import (minutes) or on send (freeform attachments). Progress shown.
- [MVP] Chunked ASR + LLM post-processing (normalization, mis-hearing fixes).
- [MVP] Retry (button / chat). In-progress transcription can be cancelled.
- [MVP] Transcript is immutable — nothing but transcribe mutates it.

### 2.7 Note organizing

- [MVP] Minutes: automatic structured write-up after transcription (HTML). Adapts to the input (decisions/actions for a meeting, organized info for a lecture, a tight summary for a monologue); length scales to the content.
- [MVP] Minutes generation runs once automatically — no auto-re-trigger.
- [MVP] Freeform: attachments are organized into the note via map-reduce with the existing note as one of the inputs; the body's first line is an `<h1>` title that spans all topics.
- [MVP] Generation-time meta is captured in `note_bodies.context_snapshot` (NOT NULL).

### 2.8 Chat agent

- [MVP] Natural-language requests in the left chat panel.
- [MVP] Tools: `write_note` (freeform), `refine_minutes` (minutes), recording download, transcribe retry, failed-task retry, transcript read (explicit request only).
- [MVP] The active note body is inlined into the prompt — content questions answered without a tool.
- [MVP] Tools are dynamically gated by stage/capability — no exposing actions the screen can't do.
- [MVP] Long-running tools run only on explicit instruction; ambiguous/status questions get a one-line suggestion.
- [MVP] Responses stream; progress is shown during a running tool.
- [MVP] A new body version adds an *Open this version* button beside the reply.
- [MVP] Correction-style input ("Sungkyunkwan") is silently substituted, not turned into a topic.
- [MVP] Hand edits survive later refinements.
- [MVP] LLM failure shows an inline red notice that clears on the next send.
- [MVP] Output language decided from `ui_lang` + the message script.

### 2.9 Lifecycle system messages

- [MVP] Record/transcribe/organize start/finish/error moments show as centered system pills in the chat.
- [MVP] Timeline is a separate table, merged chronologically; refreshed while a task is active.

### 2.10 Note history & editing

- [MVP] Expand version history; restore any version as the new active one (old versions kept).
- [MVP] *Revert to the initial state* — back to the body right after first organizing.
- [MVP] Edit the body directly (edits preserved through later refinements); copy as rich/plain text.

### 2.11 Tags & search

- [MVP] Hashtags (`#tag`) — name NOCASE unique, note M2M (CASCADE), prefix autocomplete, shown in add-order, also on note cards; orphan tags auto-cleaned.
- [MVP] Search mixes text (title/memo/location) and `#tag` chips (Enter/Space to confirm, AND-matched, ×/Backspace to remove).

### 2.12 Settings

- [MVP] AI endpoints — register/switch OpenAI-compatible ASR/LLM endpoints; `request_mode` for chat_completions/transcriptions.
- [MVP] Language — UI & AI output (KO/EN).
- [MVP] Audio — input source pick/test, OS input volume.

### 2.13 Desktop integration

- [MVP] System tray with **Open** and **Quit**; closing the window hides to the tray.

---

## 3. Non-functional

### 3.1 Data & privacy

- All data in local SQLite (app data dir). The only outbound traffic is calls to the registered AI endpoints.
- API keys are stored in local SQLite (plaintext) in v1.

### 3.2 Robustness

- Navigating away mid-recording or minimizing to the tray is safe.
- Tasks dispatch as a single transaction (task_id + `processing` row + spawn) to avoid races (G-TASK-001).
- `note_bodies.context_snapshot` is NOT NULL.
- A chat turn persists reply + tool_calls + note_body_version_id as one row.
- Failover/exception/recovery guards for long-running tasks are preserved (G-CANCEL, G-REC, …).

### 3.3 Performance & cost

- Transcription/organizing run in the background — continue across navigation and tray.
- Refinement and minutes generation are explicit-only → LLM cost control.
- List search/filter/`has_active_task`/per-note tags are server-side, one fetch per page (no N+1).

---

## 4. External integrations

| Integration | Requirement |
|---|---|
| LLM API | OpenAI-compatible. Used for post-processing, organizing, refinement, chat, and freeform map-reduce. |
| ASR API | OpenAI-compatible. Chunked calls. `transcriptions` (multipart) or audio_url style. |

---

## 5. Verification checklist

- [ ] First-run setup (language → models) works
- [ ] Create both note types; type is fixed and shown by icon
- [ ] Minutes: record/import → transcribe → generate → refine → delete
- [ ] Freeform: chat write; attach record/file/drag (multiple) → send → map-reduce merge preserves existing body
- [ ] Freeform: archive (replay/delete), unsent attachments restored on reopen
- [ ] Title follows the body's first line (no separate meta editing)
- [ ] Chat agent gates/calls tools per stage/capability; *Open this version* lands correctly
- [ ] Lifecycle pills appear chronologically; hand edits survive refinement
- [ ] Correction requests substituted silently, not turned into topics
- [ ] List search / `#tag` / date filter / pagination accurate server-side
- [ ] Settings: input source test, OS volume, AI endpoints, language
- [ ] Migrations auto-applied; installed app works without a separate ffmpeg
- [ ] Tray shows Open / Quit only
