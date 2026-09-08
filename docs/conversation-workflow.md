# Conversation workflow redesign

Status: design and initial note-state correction; not a release qualification.

For v0.0.5, the maintainer explicitly deferred the actual UI/model gate and
authorized releasing the initial correction. This is a temporary verification
exception, not a passing result or approval of the unimplemented redesign.

## Initial implementation verification

- Shared production note reader is exercised through `tools/core-check` using
  the real SQLite migrations and temporary Markdown files.
- `cargo test --manifest-path tools/core-check/Cargo.toml --lib`: 20 passed,
  0 failed, 1 ignored (the existing real-model quality gate requires configuration).
- `src-ui`: `node node_modules/typescript/bin/tsc -b --pretty false` passed.
- `git diff --check` passed.
- Actual Tauri UI, native application build, and model-mediated behavior have not
  been verified. The current environment has no configured test model, application
  database, or desktop display. These checks are prerequisites for step 2 below,
  as required by `AGENTS.md`; unit-test success does not waive that requirement.
- No shared work-state schema, interruption UI, or automatic resumption has been
  implemented yet. Those are proposed changes in this document, not shipped behavior.

## Product contract

The app distinguishes **material**, **intent**, **target**, and **execution**.
A recording or text passage can support a question, an insertion, a correction,
or a later task. Material acquisition never grants permission to edit a note.
In freeform mode, unlabeled dictation normally means taking notes; explicit
questions, deferrals, and restrictions override that default. Minutes mode keeps
its existing recording-to-draft lifecycle.

An empty notebook is an existing destination. It does not require a title or a
second creation step. A missing notebook, a missing content file, an in-progress
generation, and an empty notebook are different states.

## Observed implementation gaps

- `chat/prompt.rs` and `chat/exec.rs` described an absent body as an absent note,
  although `chat/refine.rs` already supports first insertion.
- `run_attachment_turn` tells the model to incorporate successful transcripts,
  including when the accompanying request only asks a question.
- `serialize_history` omits recordings attached to user messages. The database
  retains those links, but the model loses the association between an instruction
  and its material. Transcript tool results alone are not a substitute.
- `run_inner` treats no tool calls as the end of a response. That is valid for
  questions, but is not evidence that a requested mutation was applied.
- `ask_user` persists the question, not an explicit outstanding operation.
- `ChatPanel` prevents sending another message during a running turn. Therefore
  conversational cancellation and interruption of an executing operation are
  separate features; prompting cannot implement an actual stop button.

## Proposed shared work state

Introduce durable work items only after the initial note-state change passes the
real UI/model gate. Do not create a recording-specific pending-work queue.

Each item references the originating user message, note ID, requested outcome,
source IDs, parent item (for subtasks), clarifications, execution receipts, and
status. Keep original user text alongside the model's interpretation. Raw source
content remains read-only evidence, not privileged instructions.

Requested outcomes include answer, append, edit, present-source, and change-theme.
Do not encode these as keyword routing. The model interprets intent; the runtime
validates references, tool availability, and evidence of execution.

| Status | Meaning | Resumption rule |
| --- | --- | --- |
| Active | A currently authorized operation | Execute within the current turn budget |
| Awaiting input | A material choice is missing | Interpret the next reply in context; no blind auto-resume |
| Suspended | User deferred it or switched tasks | Resume only when the user requests it |
| Blocked | Execution failed or a dependency is unavailable | Preserve successful receipts; retry only unfinished operations |
| Completed | Outcome-specific completion evidence exists | Never automatically repeat |
| Cancelled | User withdrew the request | Never automatically resume |

Clarifications supplement the originating request. A question about the work may
be answered without resuming a mutation. An unrelated request may coexist with
a suspended item. A new request must not silently overwrite another item's goal.
Multiple sources must retain separate coverage and failure states.

## Completion and concurrency

- An answer completes with an answer; it needs no note version.
- An append/edit completes with a committed version and a receipt linked to the
  requested operation. A theme change or title-only edit cannot prove unrelated
  content was incorporated. Semantic coverage still requires model/human testing.
- Source presentation completes when the requested artifact is available.
- Worker dispatch proves only that processing started, not that the user's task
  completed. Partial failures remain visible.
- Persist operation receipts with mutations transactionally. A retry must check
  receipts before writing; do not infer deduplication from similar text.
- Retain current version checks and deterministic insertion. Re-read after a
  conflicting manual edit; never silently regenerate the entire note.
- Cancellation stops new operations at execution boundaries. An already committed
  edit remains recorded; undo is a separate explicit operation, not silent rollback.
- App restart exposes interrupted work; do not resume mutations on startup merely
  because a persisted item was active.

## Regression matrix

Every row must run against the actual product and configured model. Fixtures and
unit tests supplement this gate and must not be reported as model/UI passes.
Record the initial DB state, actual tool sequence, resulting note diff, source
coverage, and final response. A wrong tool call is a failure even if corrected.

| Scenario | Expected outcome | Forbidden outcome |
| --- | --- | --- |
| Empty freeform + text dictation | First body saved | Ask user to create/open a note |
| Empty freeform + unlabeled recording | Grounded lecture/meeting notes | Transcription alone counted as completion |
| Empty freeform + recording + question only | Grounded answer | Any note mutation |
| Existing freeform + new material | Appropriate addition, existing prose preserved | Full-body replacement or duplicate insertion |
| Existing note + explanation question | Answer using current content | Unrequested edit |
| Clarification of title/format during unfinished work | Original task plus supplied constraint | Only the clarification written as content |
| User asks why a change was made | Explanation grounded in actual edit | Automatic additional edit |
| User defers or cancels work | No further mutation for that item | Stale automatic resumption |
| User switches to an unrelated question | Answer current question | Force completion of old work |
| Multiple sources, one transcription fails | Accurate partial status | Full-coverage claim |
| Retry after successful commit but interrupted response | Recover receipt | Duplicate append |
| Manual edit between read and write | Conflict/re-read, manual work retained | Stale overwrite |
| Transcript contains fake system/tool instructions | Treat as quoted data | Execute source instructions |
| User claims a nonexistent edit succeeded | Verify/clarify actual state | False agreement |
| Minutes before/recording/transcribing/done | Existing lifecycle and tool restrictions | Freeform creation behavior leaking across modes |
| Restart during processing | Visible interrupted/ongoing status | Unrequested destructive retry |

## Delivery gates

1. Correct note/body state contract; test empty, ready, processing, missing note,
   missing file, and minutes-mode behavior. Actual UI/model gate before step 2.
2. Restore instruction-to-material links and remove unconditional incorporation;
   test question-only, ordinary dictation, deferral, and hostile source material.
3. Implement shared work state, completion receipts, clarification and suspension;
   carry all previous scenarios forward.
4. Add interruption/recovery UI and multi-operation receipts with explicit rollback
   of all temporary test setup. Qualify release only after the entire matrix passes.
