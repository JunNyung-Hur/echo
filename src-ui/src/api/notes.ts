import { invoke } from "@tauri-apps/api/core";
import { Tag } from "./tags";

export interface Note {
  id: string;
  title: string;
  description: string | null;
  location: string | null;
  language: string;
  started_at: string | null;
  source_type: string;
  /** "minutes" | "freeform" — null = 미선택(진입 시 유형 선택). 선택 후 고정. */
  note_type: string | null;
  created_at: string;
  updated_at: string;
}

export interface NoteListItem {
  id: string;
  title: string;
  description: string | null;
  location: string | null;
  started_at: string | null;
  note_type: string | null;
  created_at: string;
  updated_at: string;
  has_active_task: number;
  /** Tags on this note (filled by list_notes). */
  tags?: Tag[];
}

export interface ListNotesResponse {
  items: NoteListItem[];
  total: number;
  page: number;
  page_size: number;
}

export interface CreateNoteInput {
  title?: string | null;
  description?: string | null;
  location?: string | null;
  language?: string | null;
  started_at?: string | null;
  note_type?: string;
}

export interface UpdateNoteInput {
  title?: string;
  description?: string | null;
  location?: string | null;
  language?: string;
  started_at?: string | null;
  note_type?: string;
}

export interface ListNotesQuery {
  q?: string | null;
  from_date?: string | null;
  to_date?: string | null;
  /** #tag tokens parsed from the search box — each must be present (AND). */
  tag_names?: string[];
  page?: number;
  page_size?: number;
}

export const notesApi = {
  create: (input: CreateNoteInput) => invoke<Note>("create_note", { input }),
  list: (query: ListNotesQuery) => invoke<ListNotesResponse>("list_notes", { query }),
  get: (id: string) => invoke<Note>("get_note", { id }),
  update: (id: string, input: UpdateNoteInput) => invoke<Note>("update_note", { id, input }),
  delete: (id: string) => invoke<void>("delete_note", { id }),
};
