# Core and note quality checks

This crate imports production source files directly. It tests HTTP/SSE handling,
Unicode, evidence retrieval, note-scoped source access, insertion preservation,
version conflicts and retry state transitions using deterministic wire fixtures
and a real temporary SQLite database. The error adapter excludes only the GUI
error variant. It does not simulate a model or claim to test the desktop UI.

```sh
cargo test --locked --manifest-path tools/core-check/Cargo.toml --lib
```

Actual desktop compilation and its existing tests run separately in
`.github/workflows/quality.yml`. Real desktop UI and configured ASR/LLM scenarios
remain required before release, as specified in `AGENTS.md`.

## Compare note quality

Set `ECHO_EVAL_URL` (base URL including `/v1`), `ECHO_EVAL_MODEL`, and optionally
`ECHO_EVAL_KEY` in your environment. Never commit credentials. Then run:

```sh
cargo test --locked --manifest-path tools/core-check/Cargo.toml --lib real_note_quality_gate -- --ignored --nocapture
```

This sends three real requests: a conditional decision, a correction with
attribution, and an adversarial recording. All scenarios must pass. Reports
under `target/quality/` include generated notes, elapsed time, token usage and
human review criteria; credentials are not written. Keyword checks are only a
smoke gate: passing them does not establish factual completeness or quality.
Review every output against `human_checks`, especially negation and conditions.

Use the same recordings and fixed gold transcripts to compare old/new model ×
old/new pipeline. Blindly rate important-fact recall, unsupported claims,
owner/deadline accuracy, conditions, edit preservation and usefulness. Record
latency and tokens separately. Run the ASR versions against the same audio to
separate transcription errors from note-writing errors. Add 10–20 actual problem
cases before claiming a dominant improvement; the bundled fixtures are synthetic.

## Desktop scenarios still required

1. Minutes: ask why a decision was conditional without mentioning “transcript”.
   Verify source retrieval, the answer, and an evidence-backed edit in the real UI.
2. Freeform: attach multiple recordings, add a related memo, then correct a name
   and request organization. Verify unrelated content, conditions and versions.
3. Adversarial: false-premise question and an instruction inside a recording.
   Verify no invented agreement or unauthorized edit.
4. Cause one ASR chunk to fail. Verify no completed transcript/note is published,
   the missing interval is visible, and retry calls only uncached chunks.
5. Race a manual edit with an agent edit. Verify stale output cannot overwrite
   the new version. Exercise empty, populated and legacy HTML notes.
6. Test both Korean and English UI modes, each endpoint request mode, output
   truncation, cancellation, failed attachments and reconnect/retry.

Use temporary test notes and remove them afterward. A tool-intent mismatch fails
the scenario even if the final response sounds correct.
