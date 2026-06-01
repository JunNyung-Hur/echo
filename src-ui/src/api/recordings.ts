import { invoke } from "@tauri-apps/api/core";

export interface Recording {
  id: string;
  note_id: string;
  file_path: string;
  original_filename: string;
  duration: number | null;
  format: string;
  last_chunk_at: string | null;
  finalized_at: string | null;
  /** Set when a chat send consumed this attachment; null = pending chip. */
  consumed_at: string | null;
  created_at: string;
}

export const recordingsApi = {
  listForNote: (noteId: string) => invoke<Recording[]>("list_recordings", { noteId }),
  /** Finalized recordings attached but not yet sent — restored as chips. */
  listPending: (noteId: string) => invoke<Recording[]>("list_pending_recordings", { noteId }),
  /** Recordings already sent — the 보관함(archive) history. */
  listArchived: (noteId: string) => invoke<Recording[]>("list_archived_recordings", { noteId }),
  get: (id: string) => invoke<Recording>("get_recording", { id }),
  /** Raw audio bytes (ArrayBuffer) for in-app playback via a Blob URL. */
  readAudio: (id: string) => invoke<ArrayBuffer>("read_recording_audio", { id }),
  /** Native cpal capture (D-023). source: "mic" | "system". */
  start: (noteId: string, deviceName: string, source: "mic" | "system") =>
    invoke<Recording>("start_recording", { noteId, deviceName, source }),
  stop: (recordingId: string) => invoke<void>("stop_recording", { recordingId }),
  delete: (recordingId: string) => invoke<void>("delete_recording", { recordingId }),
  /** F-REC-004 — import an external audio file (converted to webm, then the
   *  transcribe chain auto-runs). `srcPath` is an absolute filesystem path. */
  importAudioFile: (noteId: string, srcPath: string) =>
    invoke<Recording>("import_audio_file", { noteId, srcPath }),
};
