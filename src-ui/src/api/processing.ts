import { invoke } from "@tauri-apps/api/core";

/** transcripts row (subset the UI needs). status: pending/processing/completed/failed/cancelled/empty */
export interface Transcript {
  id: string;
  note_id: string;
  recording_id: string | null;
  status: string;
  created_at: string;
}

/** note_bodies row (subset). status: pending/processing/completed/failed */
export interface NoteBody {
  id: string;
  note_id: string;
  status: string;
  content_path: string | null;
  /** 0 = active version, 1 = archived (superseded by a refine/restore). */
  archived: number;
  /** 1 = produced by a manual edit (vs AI generate/refine). */
  is_manual_edit: number;
  /** JSON snapshot of note meta at body-generation time (F-NOTE-004 compare). */
  context_snapshot: string;
  created_at: string;
}

/** note_timeline_events row — lifecycle marks shown as chat pills. */
export interface TimelineEvent {
  id: string;
  note_id: string;
  kind: string;
  content: string;
  created_at: string;
}

export const processingApi = {
  listTranscripts: (noteId: string) => invoke<Transcript[]>("list_transcripts", { noteId }),
  /** F-VIEW (1f207ab) — full transcript text by id, for the 전체보기 modal. */
  getTranscriptContent: (transcriptId: string) =>
    invoke<string>("get_transcript_content", { transcriptId }),
  listNoteBodies: (noteId: string) => invoke<NoteBody[]>("list_note_bodies", { noteId }),
  listTimeline: (noteId: string) => invoke<TimelineEvent[]>("list_timeline", { noteId }),
  /** Rendered minutes HTML for a body (read from disk), or null if not ready. */
  getBodyContent: (bodyId: string) => invoke<string | null>("get_body_content", { bodyId }),
  /** F-TRANS-005 — clean prior artifacts + re-run the chain from the recording. */
  retryTranscribe: (noteId: string) => invoke<void>("retry_transcribe", { noteId }),
  /** F-VERSION-001 — restore an archived body version as a new active one. */
  restoreBody: (noteId: string, bodyId: string) =>
    invoke<void>("restore_note_body", { noteId, bodyId }),
  /** F-VIEW — save a manual edit of the body as a new (직접 수정) version. */
  saveManualEdit: (noteId: string, html: string) =>
    invoke<void>("save_manual_body_edit", { noteId, html }),
};
