# Agent Instructions

## Documentation

- Do not add secrets, credentials, internal URLs, or personal data to `docs/public/`.

## Release Work

- When explicitly preparing a release, update the relevant public docs, including `docs/public/release-notes/<version>.md`.

## Implementation & Verification

For non-trivial implementation steps (especially agent / LLM work, multi-stage features, or anything where behaviour depends on stateful integration), apply these ground rules. The failure mode this section exists to prevent is silently skipping verification and propagating broken assumptions to the next sub-step.

- **E2E verify each sub-step before moving on.** Don't chain multiple sub-steps with only static checks; the next sub-step inherits broken assumptions from the previous.
- **Drive the real UI, not a mock.** Walk the actual product flow end-to-end. Static tests are supplementary.
- **Set up missing preconditions explicitly, with rollback.** If the verification path requires an unreachable state (e.g. a `failed` transcript, a stale minutes version, a specific stage), force it via temporary code, a DB row insert, or browser-side state override — and add the change to a rollback checklist for that sub-step. Tear it down at the sub-step's close. "I'll remember to undo this" is the most common slip.
- **For any LLM-mediated behaviour, run ≥3 distinct scenarios.** A single happy-path run can pass by coincidence. Vary scenarios along independent axes — current `stage`, intent shape of user utterance (plain / ambiguous / adversarial), meeting state (happy path / mid-progress / failed / edge), and expected outcome (which tool must be called, which keywords must / must not appear). Three scenarios that share a single axis don't count as three. **At least one of the three must be adversarial** — a false premise, an out-of-scope demand, or an instruction that conflicts with current state. Plain + ambiguous + plain is not enough; the adversarial axis is where the LLM is most likely to fold and produce false agreement.
- **≥3 is a floor, not a ceiling.** When the sub-step introduces a schema or data structure with multiple axes (a state object with several fields, a registry with multiple action categories, a flag with several values), keep adding scenarios until every axis has at least one scenario that meaningfully exercises it.
- **Intent ↔ tool-call mismatch is a FAIL, not a "with-caveat".** If the LLM invokes a tool that doesn't match what the user actually asked for, the scenario is **FAIL**, regardless of how the conversation ends up after the wrong tool returns. Don't be generous with PASS stamps when the root signal — tool selection — is wrong.
- **Carry past scenarios forward as regression cases.** Once a scenario is verified for a sub-step, it stays in scope for every later sub-step in the same effort. Re-run it; new code should not regress earlier behaviour.
- **Fail-and-investigate, not fail-and-stop.** A scenario failure is an investigation trigger — debug, retry, fix. Continue iterating while progress is visible. Only stop and surface state to the user when repeated attempts can't resolve the failure and the sub-step is genuinely stuck.

**Dev-env auth shortcut for E2E.** When the mock IDP login picker appears during automated E2E, default to the **admin (admin@test.com)** account. It covers the broader capability surface (admin-only UI / role-gated tools) and is the baseline for verification. Drop down to a specific user only when a scenario explicitly needs the non-admin role.

**Suspect the automation harness before declaring a UI regression.** If a tool-driven click via element ref produces no visible effect (no navigation, no DOM change), the cause is almost always that the ref-based click did not dispatch a real synthetic event that React's onClick handler picks up — *not* a bug in the product. Cross-check with a coordinate-based click before concluding the button is broken.

**Batch tool calls aggressively; don't cut at every dependent action.** Bundle a natural user-action unit into a single batch — typically `form_input` + `click send` + `wait` + `screenshot` — even when one step's output (e.g. an element ref) feeds the next. Only split when the *next* action genuinely depends on inspecting the previous result (e.g. PASS → next scenario, FAIL → debug). The same applies to non-browser tool calls: chain independent reads in parallel; chain a planned sequence in one call when the chain is straightforward.

## Autonomous Progress

Default to autonomous progress. Asking, stopping, and "waiting for approval" each cost the user's time and attention — do them only when genuinely necessary. (This section is the counterpart to Implementation & Verification: that one governs *how rigorously* you check, this one governs *how far you carry the work without interrupting*.)

- **Don't ask about decisions you can make yourself.** Order of work, names, sub-conventions, optional flags — anything that doesn't change the goal (or is easily reversible) is yours to decide. Pick a sensible default, proceed, and report it inline ("went with X because Y; say if you'd rather Z"). Only stop at genuinely critical forks: identity, architecture, stack, domain model, data-loss / retention policy.
- **Don't stop at natural seams.** Finishing one file / slice / sub-step does not end the turn — roll straight into the next pending item, keeping progress notes to one line ("X done, on to Y"). The only valid stops: (a) the work list is empty, (b) a critical fork, (c) an explicit user stop ("멈춰" / "잠깐" / "stop"). A token-limit split is fine — that's hitting a ceiling, not announcing an early exit.
- **Phase / milestone transitions are autonomous too.** If the phase's checks are clean, start the next phase's first task and keep going. Stop only on a real doubt signal (scenario FAIL, build error, suspected missing guard).
- **Watch the "let me just confirm" pull — it grows with task size.** The bigger or more daunting the work looks, the stronger the urge to grab one approval first. That urge is not safety; it is deferring the work. Noticing it on a *non-critical* decision is itself the signal to decide and proceed — not to ask.
