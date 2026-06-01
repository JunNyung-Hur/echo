// Stage derivation — 1:1 with the old stack's `derive_stage`
// (system_prompt.py). Priority order, G-STAGE-001:
//   done > recording > transcribing > before
//
// Phase 2 wires transcripts + note_bodies in (Phase 1 saw recordings only).

import { Recording } from "@/api/recordings";
import { Transcript, NoteBody } from "@/api/processing";

export type Stage = "before" | "recording" | "transcribing" | "done";

export interface StageInputs {
  recordings: Recording[];
  transcripts: Transcript[];
  noteBodies: NoteBody[];
}

export function deriveStage({ recordings, transcripts, noteBodies }: StageInputs): Stage {
  // done — an *active* (non-archived) completed body exists (G-DB-004 keeps it
  // unique). After a refine, the old body lingers as archived+completed, so the
  // archived check is required — not just status.
  if (noteBodies.some((b) => b.archived === 0 && b.status === "completed")) return "done";

  // recording — capture still live.
  if (recordings.some((r) => r.format === "recording" || r.format === "finalizing")) {
    return "recording";
  }

  // transcribing — transcribe/generate in flight, OR a finalized recording is
  // waiting on (or failed in) the pipeline. The panel differentiates
  // in-progress vs failed by inspecting statuses.
  const inFlight =
    transcripts.some((t) => t.status === "pending" || t.status === "processing") ||
    noteBodies.some((b) => b.status === "pending" || b.status === "processing");
  if (inFlight || recordings.length > 0) return "transcribing";

  return "before";
}
