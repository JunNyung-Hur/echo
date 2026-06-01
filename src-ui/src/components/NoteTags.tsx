import { useEffect, useRef, useState } from "react";
import { Plus, X } from "lucide-react";

import { tagsApi, Tag } from "@/api/tags";
import { useT } from "@/i18n/LangContext";

/**
 * Tag chips for a note (F-TAG-001/002/003) — shown in the chat-panel header so
 * tags stay editable across every stage. Each chip removes on hover-X; the "+"
 * opens an inline input with name-prefix autocomplete and a create-new row.
 */
export default function NoteTags({ noteId }: { noteId: string }) {
  const t = useT();
  const [tags, setTags] = useState<Tag[]>([]);
  const [adding, setAdding] = useState(false);
  const [draft, setDraft] = useState("");
  const [suggestions, setSuggestions] = useState<Tag[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);
  const boxRef = useRef<HTMLDivElement>(null);

  const load = () => {
    tagsApi.forNote(noteId).then(setTags).catch(() => {});
  };
  useEffect(() => {
    load();
    setAdding(false);
    setDraft("");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [noteId]);

  // Autocomplete — hide tags already on the note.
  useEffect(() => {
    if (!adding) return;
    const q = draft.trim();
    if (!q) {
      setSuggestions([]);
      return;
    }
    let alive = true;
    tagsApi
      .suggest(q)
      .then((s) => {
        if (alive) setSuggestions(s.filter((x) => !tags.some((tg) => tg.id === x.id)));
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [draft, adding, tags]);

  // Close the inline editor on outside click.
  useEffect(() => {
    if (!adding) return;
    const onDoc = (e: MouseEvent) => {
      if (boxRef.current && !boxRef.current.contains(e.target as Node)) closeAdd();
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [adding]);

  const openAdd = () => {
    setAdding(true);
    setDraft("");
    setTimeout(() => inputRef.current?.focus(), 0);
  };
  const closeAdd = () => {
    setAdding(false);
    setDraft("");
    setSuggestions([]);
  };

  const addTag = async (name: string) => {
    const n = name.trim();
    if (!n) return;
    try {
      await tagsApi.add(noteId, n);
      setDraft("");
      setSuggestions([]);
      load();
      inputRef.current?.focus();
    } catch {
      // ignore — duplicate names resolve server-side (G-TAG-001).
    }
  };

  const removeTag = async (tagId: string) => {
    try {
      await tagsApi.remove(noteId, tagId);
      setTags((prev) => prev.filter((x) => x.id !== tagId));
    } catch {
      // ignore
    }
  };

  // Suppress the "create" row when the typed name already matches a suggestion.
  const exact = suggestions.some((s) => s.name.toLowerCase() === draft.trim().toLowerCase());

  return (
    <div ref={boxRef} className="relative flex flex-wrap items-center gap-2 mt-2">
      {tags.map((tg) => (
        <span
          key={tg.id}
          className="group/tag relative inline-flex items-center h-5 text-[12px] text-sky-600 leading-none"
        >
          #{tg.name}
          <button
            onClick={() => removeTag(tg.id)}
            className="absolute -top-1.5 -right-2 opacity-0 group-hover/tag:opacity-100 transition-opacity w-3.5 h-3.5 flex items-center justify-center rounded-full bg-sky-500 text-white hover:bg-sky-600 border border-white shadow-sm cursor-pointer p-0"
            title={t("tags.remove")}
            aria-label={t("tags.remove")}
          >
            <X size={8} strokeWidth={3} />
          </button>
        </span>
      ))}

      {adding ? (
        <span className="relative inline-flex items-center h-5 text-[12px] text-sky-600 leading-none">
          <span className="text-sky-600">#</span>
          <span className="relative inline-block h-full w-20">
            <input
              ref={inputRef}
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  addTag(draft);
                } else if (e.key === "Escape") {
                  closeAdd();
                }
              }}
              placeholder={t("tags.addPlaceholder")}
              className="absolute inset-0 w-full h-full p-0 bg-transparent border-0 outline-none text-[12px] leading-none text-sky-600 placeholder:text-sky-300"
            />
          </span>
          {(suggestions.length > 0 || (draft.trim() !== "" && !exact)) && (
            <div className="absolute left-0 top-full mt-1 z-30 bg-white border border-gray-200 rounded-lg shadow-lg py-1 w-44 max-h-48 overflow-y-auto">
              {suggestions.map((s) => (
                <button
                  key={s.id}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    addTag(s.name);
                  }}
                  className="block w-full text-left px-2.5 py-1 text-[11px] text-gray-700 hover:bg-gray-50 bg-transparent border-0 cursor-pointer truncate"
                >
                  {s.name}
                </button>
              ))}
              {draft.trim() !== "" && !exact && (
                <button
                  onMouseDown={(e) => {
                    e.preventDefault();
                    addTag(draft);
                  }}
                  className="flex items-center gap-1 w-full text-left px-2.5 py-1 text-[11px] text-sky-700 hover:bg-sky-50 bg-transparent border-0 cursor-pointer truncate"
                >
                  <Plus size={10} className="shrink-0" />
                  {t("tags.create", { name: draft.trim() })}
                </button>
              )}
            </div>
          )}
        </span>
      ) : (
        <button
          onClick={openAdd}
          className="inline-flex items-center gap-0.5 h-5 text-[12px] leading-none text-sky-600 hover:text-sky-700 bg-transparent border-0 cursor-pointer transition-colors"
          title={t("tags.add")}
        >
          <Plus size={11} />
          <span>{t("tags.add")}</span>
        </button>
      )}
    </div>
  );
}
