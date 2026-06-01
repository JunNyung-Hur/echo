import { invoke } from "@tauri-apps/api/core";

/** A tag row. */
export interface Tag {
  id: string;
  name: string;
  color: string | null;
  created_at: string;
}

/** A tag plus how many notes carry it — drives the filter sidebar. */
export interface TagWithCount extends Tag {
  usage: number;
}

export const tagsApi = {
  /** All tags + usage counts (F-TAG-004 filter sidebar). */
  list: () => invoke<TagWithCount[]>("list_tags"),
  /** Tags attached to one note (chips on the note view). */
  forNote: (noteId: string) => invoke<Tag[]>("list_note_tags", { noteId }),
  /** Name-prefix autocomplete (F-TAG-003). */
  suggest: (prefix: string) => invoke<Tag[]>("suggest_tags", { prefix }),
  /** Create-or-reuse a tag by name and attach it to a note in one call. */
  add: (noteId: string, name: string) => invoke<Tag>("add_note_tag", { noteId, name }),
  /** Detach a tag from a note (the tag survives for other notes). */
  remove: (noteId: string, tagId: string) => invoke<void>("remove_note_tag", { noteId, tagId }),
  /** Rename a tag everywhere it's used. */
  rename: (id: string, name: string) => invoke<Tag>("rename_tag", { id, name }),
  /** Delete a tag globally; note_tags cascade. */
  delete: (id: string) => invoke<void>("delete_tag", { id }),
};
