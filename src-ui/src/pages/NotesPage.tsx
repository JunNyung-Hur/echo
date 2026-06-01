import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import {
  Plus,
  Search,
  MapPin,
  Clock,
  ArrowRight,
  ChevronDown,
  Sparkles,
  Loader2,
  Calendar,
  X,
  Settings,
  PenLine,
  ClipboardList,
  FileText,
  Mic,
} from "lucide-react";

import { notesApi, NoteListItem } from "@/api/notes";
import { useLang, type TFunc } from "@/i18n/LangContext";
import { systemApi } from "@/api/system";
import {
  daysAgo,
  dateKey,
  shortDate,
  formatDateLong,
  formatTime,
  todayIso,
  roundTo5Min,
  formatDateRangeLabel,
  greetingFor,
} from "@/lib/datetime";

const PAGE_SIZE = 20;

interface AgendaGroup {
  label: string;
  sub: string | null;
  items: NoteListItem[];
}

function groupByDate(list: NoteListItem[], now: Date, t: TFunc, lang: "ko" | "en"): AgendaGroup[] {
  const groups: AgendaGroup[] = [];
  const seen = new Map<string, number>();
  for (const n of list) {
    const d = new Date(n.started_at || n.created_at);
    const key = dateKey(d);
    let idx = seen.get(key);
    if (idx === undefined) {
      idx = groups.length;
      seen.set(key, idx);
      const gap = daysAgo(d, now);
      const label =
        gap === 0 ? t("notes.group.today") : gap === 1 ? t("notes.group.yesterday") : shortDate(d, lang);
      const sub = gap === 0 || gap === 1 ? shortDate(d, lang) : null;
      groups.push({ label, sub, items: [] });
    }
    groups[idx].items.push(n);
  }
  return groups;
}

/** Split the search box into #tag tokens (AND-matched) and free text. */
function parseSearch(input: string): { text: string; tags: string[] } {
  const tags: string[] = [];
  const words: string[] = [];
  for (const tok of input.split(/\s+/)) {
    if (tok.startsWith("#")) {
      // "#word" → tag; a lone "#" (still typing) is neither text nor tag.
      if (tok.length > 1) tags.push(tok.slice(1));
    } else if (tok) {
      words.push(tok);
    }
  }
  return { text: words.join(" "), tags };
}

export default function NotesPage() {
  const navigate = useNavigate();
  const { t, lang } = useLang();
  const [username, setUsername] = useState<string | null>(null);
  useEffect(() => {
    systemApi.getUsername().then(setUsername).catch(() => setUsername(null));
  }, []);

  // Cached layer (no q). Never mutated by search.
  const [notes, setNotes] = useState<NoteListItem[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);

  // Search layer. Toggled visibility only — never unmounts.
  const [searchResults, setSearchResults] = useState<NoteListItem[]>([]);
  const [searchTotal, setSearchTotal] = useState(0);
  const [searchPage, setSearchPage] = useState(1);
  // Confirmed #tag chips in the search box (AND-matched).
  const [searchTags, setSearchTags] = useState<string[]>([]);

  // Date range filter
  const [dateFrom, setDateFrom] = useState("");
  const [dateTo, setDateTo] = useState("");
  const dateChipActive = !!(dateFrom || dateTo);
  const [datePopupOpen, setDatePopupOpen] = useState(false);
  const datePopupRef = useRef<HTMLDivElement>(null);
  const prefilledRef = useRef(false);

  const openDatePopup = useCallback(() => {
    setDatePopupOpen(true);
    if (!dateFrom && !dateTo) {
      setDateTo(todayIso());
      prefilledRef.current = true;
    }
  }, [dateFrom, dateTo]);
  const closeDatePopup = useCallback(() => {
    setDatePopupOpen(false);
    if (prefilledRef.current && !dateFrom) setDateTo("");
    prefilledRef.current = false;
  }, [dateFrom]);
  useEffect(() => {
    if (!datePopupOpen) return;
    const handler = (e: MouseEvent) => {
      if (datePopupRef.current && !datePopupRef.current.contains(e.target as Node)) {
        closeDatePopup();
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [datePopupOpen, closeDatePopup]);

  const [searchSettled, setSearchSettled] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [creating, setCreating] = useState(false);
  const [typePickerOpen, setTypePickerOpen] = useState(false);
  const [query, setQuery] = useState("");

  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 60_000);
    return () => clearInterval(t);
  }, []);

  const [debouncedQuery, setDebouncedQuery] = useState("");
  useEffect(() => {
    const handle = window.setTimeout(() => {
      const next = query.trim();
      if (next.length === 1) return;
      setDebouncedQuery(next);
    }, 300);
    return () => window.clearTimeout(handle);
  }, [query]);

  const isSearchMode = debouncedQuery !== "" || searchTags.length > 0;
  const [searching, setSearching] = useState(false);

  const fetchCached = useCallback(
    async (p: number, append: boolean, from: string, to: string) => {
      const resp = await notesApi.list({
        page: p,
        page_size: PAGE_SIZE,
        from_date: from || null,
        to_date: to || null,
      });
      setTotal(resp.total);
      setNotes((prev) => (append ? [...prev, ...resp.items] : resp.items));
      setLoaded(true);
    },
    [],
  );

  const fetchSearch = useCallback(
    async (p: number, append: boolean, rawQuery: string, tags: string[], from: string, to: string) => {
      const { text } = parseSearch(rawQuery);
      const resp = await notesApi.list({
        page: p,
        page_size: PAGE_SIZE,
        q: text || null,
        tag_names: tags.length ? tags : undefined,
        from_date: from || null,
        to_date: to || null,
      });
      setSearchTotal(resp.total);
      setSearchResults((prev) => (append ? [...prev, ...resp.items] : resp.items));
    },
    [],
  );

  useEffect(() => {
    setPage(1);
    fetchCached(1, false, dateFrom, dateTo).catch((e) => toast.error(String(e)));
  }, [fetchCached, dateFrom, dateTo]);

  useEffect(() => {
    if (!isSearchMode) {
      setSearchSettled(false);
      return;
    }
    setSearchPage(1);
    setSearching(true);
    setSearchSettled(false);
    fetchSearch(1, false, debouncedQuery, searchTags, dateFrom, dateTo)
      .catch((e) => toast.error(String(e)))
      .finally(() => {
        setSearching(false);
        setSearchSettled(true);
      });
  }, [debouncedQuery, isSearchMode, fetchSearch, dateFrom, dateTo, searchTags]);

  const searchInputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        searchInputRef.current?.focus();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, []);

  // 새 노트 → 유형을 먼저 고르고(취소 가능) 그 시점에 생성. 유형 선택 전엔 노트를 만들지 않는다.
  const openTypePicker = () => setTypePickerOpen(true);
  const createWithType = async (noteType: "freeform" | "minutes") => {
    if (creating) return;
    setCreating(true);
    try {
      const note = await notesApi.create({
        title: null,
        description: null,
        location: null,
        started_at: roundTo5Min(new Date()).toISOString(),
        note_type: noteType,
      });
      navigate(`/notes/${note.id}`);
    } catch (e) {
      toast.error(String(e));
      setCreating(false);
    }
  };

  const cachedGrouped = useMemo(() => groupByDate(notes, now, t, lang), [notes, now, t, lang]);
  const searchGrouped = useMemo(
    () => groupByDate(searchResults, now, t, lang),
    [searchResults, now, t, lang],
  );

  const hasNarrowed = isSearchMode || dateChipActive;
  const noNotes = loaded && total === 0 && !hasNarrowed;
  const hasMoreCached = notes.length < total;
  const hasMoreSearch = searchResults.length < searchTotal;

  if (!loaded) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center gap-3 min-h-screen">
        <div className="w-8 h-8 border-[3px] border-sky-200 border-t-sky-600 rounded-full animate-spin" />
        <p className="text-xs text-gray-400">{t("notes.loading")}</p>
      </div>
    );
  }

  if (noNotes) {
    return (
      <div className="flex-1 flex flex-col min-h-screen" style={{ paddingBottom: 56 }}>
        <div className="flex-1 flex items-center justify-center">
          <div className="text-center px-6">
            <div className="relative inline-flex items-center justify-center mb-8">
              <span className="absolute inset-0 -m-6 rounded-full bg-sky-100/40 blur-2xl" />
              <span className="absolute inset-0 -m-2 rounded-full bg-sky-50" />
              <span className="relative w-20 h-20 rounded-full bg-white shadow-sm border border-sky-100 flex items-center justify-center">
                <Sparkles className="w-[26px] h-[26px] text-sky-500" strokeWidth={1.75} />
              </span>
            </div>
            <h2 className="text-xl font-semibold text-gray-900 mb-2">{t("notes.empty.title")}</h2>
            <p className="text-sm text-gray-500 mb-8 leading-relaxed whitespace-pre-line">
              {t("notes.empty.desc")}
            </p>
            <button
              onClick={openTypePicker}
              disabled={creating}
              className="inline-flex items-center gap-2 px-6 py-2.5 bg-sky-600 text-white rounded-full hover:bg-sky-700 transition-all text-sm font-medium shadow-md cursor-pointer border-0 disabled:opacity-60 disabled:cursor-not-allowed"
            >
              <Plus className="w-3.5 h-3.5" />
              {t("notes.empty.create")}
            </button>
          </div>
        </div>
        {typePickerOpen && (
          <NewNoteTypeModal
            onPick={createWithType}
            onClose={() => setTypePickerOpen(false)}
            disabled={creating}
          />
        )}
      </div>
    );
  }

  return (
    <div className="w-full px-6 py-8 min-h-screen">
      <div className="max-w-3xl mx-auto w-full">
        {/* Greeting hero */}
        <div className="mb-7 pt-2 flex items-start justify-between">
          <div>
            <p className="text-[11px] text-gray-400 mb-1 tracking-wide">{formatDateLong(now, lang)}</p>
            <h1 className="text-lg font-medium tracking-tight text-gray-800 leading-tight">
              {greetingFor(now, username, lang)}
            </h1>
          </div>
          <button
            onClick={() => navigate("/settings")}
            title={t("notes.settings")}
            className="p-2 rounded-full text-gray-400 hover:text-gray-700 hover:bg-white/60 transition-colors bg-transparent border-0 cursor-pointer"
          >
            <Settings className="w-4 h-4" />
          </button>
        </div>

        {/* Action row */}
        <div className="flex items-center gap-2.5 mb-8">
          <button
            onClick={openTypePicker}
            disabled={creating}
            className="flex items-center gap-1.5 px-5 py-2.5 bg-sky-600 text-white rounded-full hover:bg-sky-700 transition-all text-sm font-medium shadow-md cursor-pointer border-0 shrink-0 disabled:opacity-60 disabled:cursor-not-allowed"
          >
            <Plus className="w-3.5 h-3.5" />
            {t("notes.new")}
          </button>
          <div
            className="flex-1 flex flex-wrap items-center gap-x-2 gap-y-1 bg-white border border-gray-200 rounded-full px-4 py-2 hover:border-gray-300 transition-colors focus-within:border-sky-300 min-w-0 cursor-text"
            onClick={() => searchInputRef.current?.focus()}
          >
            <Search className="w-3.5 h-3.5 text-gray-400 shrink-0" />
            {searchTags.map((tag, i) => (
              <span
                key={`${tag}-${i}`}
                className="inline-flex items-center text-[13px] text-sky-600 leading-none"
              >
                #{tag}
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    setSearchTags((prev) => prev.filter((_, j) => j !== i));
                  }}
                  className="ml-0.5 text-sky-400 hover:text-sky-700 bg-transparent border-0 cursor-pointer p-0 inline-flex items-center"
                  aria-label={t("tags.remove")}
                >
                  <X className="w-3 h-3" />
                </button>
              </span>
            ))}
            <input
              ref={searchInputRef}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  const m = query.match(/(^|\s)(#\S+)\s*$/);
                  if (m) {
                    e.preventDefault();
                    const tag = m[2].replace(/^#/, "").trim();
                    if (tag) setSearchTags((prev) => [...prev, tag]);
                    setQuery(query.slice(0, m.index ?? 0).replace(/\s+$/, ""));
                  }
                } else if (e.key === "Backspace" && query === "" && searchTags.length > 0) {
                  setSearchTags((prev) => prev.slice(0, -1));
                }
              }}
              placeholder={searchTags.length > 0 ? "" : t("notes.search.placeholder")}
              className="flex-1 min-w-[6rem] text-sm text-gray-700 bg-transparent border-0 outline-none placeholder:text-gray-400"
            />
            {searching && <Loader2 className="w-3.5 h-3.5 text-gray-400 shrink-0 animate-spin" />}
          </div>
          <div ref={datePopupRef} className="relative shrink-0">
            <button
              onClick={() => (datePopupOpen ? closeDatePopup() : openDatePopup())}
              className={`flex items-center gap-1.5 px-4 py-2 rounded-full text-sm border transition-colors cursor-pointer ${
                dateChipActive
                  ? "border-sky-200 bg-sky-50 text-sky-700"
                  : "border-gray-200 bg-white text-gray-600 hover:border-gray-300"
              }`}
            >
              <Calendar className="w-3.5 h-3.5" />
              <span>{dateChipActive ? formatDateRangeLabel(dateFrom, dateTo) : t("notes.dateFilter")}</span>
              {dateChipActive && (
                <span
                  role="button"
                  tabIndex={0}
                  onClick={(e) => {
                    e.stopPropagation();
                    setDateFrom("");
                    setDateTo("");
                    setDatePopupOpen(false);
                  }}
                  className="-mr-1 p-0.5 text-sky-700 hover:text-sky-900 cursor-pointer"
                  aria-label={t("notes.dateFilter.clear")}
                >
                  <X className="w-3.5 h-3.5" />
                </span>
              )}
            </button>
            {datePopupOpen && (
              <div className="absolute right-0 top-full mt-1.5 w-72 bg-white border border-gray-100 rounded-xl shadow-lg p-3 z-50">
                <div className="space-y-2">
                  <label className="block">
                    <span className="text-[11px] text-gray-500 mb-1 block">{t("notes.dateFilter.from")}</span>
                    <input
                      type="date"
                      value={dateFrom}
                      max={dateTo || undefined}
                      onChange={(e) => {
                        setDateFrom(e.target.value);
                        prefilledRef.current = false;
                      }}
                      className="w-full text-sm text-gray-700 border border-gray-200 rounded-md px-2 py-1.5 outline-none focus:border-sky-300"
                    />
                  </label>
                  <label className="block">
                    <span className="text-[11px] text-gray-500 mb-1 block">{t("notes.dateFilter.to")}</span>
                    <input
                      type="date"
                      value={dateTo}
                      min={dateFrom || undefined}
                      onChange={(e) => {
                        setDateTo(e.target.value);
                        prefilledRef.current = false;
                      }}
                      className="w-full text-sm text-gray-700 border border-gray-200 rounded-md px-2 py-1.5 outline-none focus:border-sky-300"
                    />
                  </label>
                  <p className="text-[10px] text-gray-400 leading-snug">
                    {t("notes.dateFilter.hint")}
                  </p>
                </div>
                <div className="flex items-center justify-between mt-3 pt-2 border-t border-gray-100">
                  <button
                    onClick={() => {
                      setDateFrom("");
                      setDateTo("");
                      prefilledRef.current = false;
                    }}
                    className="text-xs text-gray-500 hover:text-gray-900 bg-transparent border-0 cursor-pointer"
                  >
                    {t("notes.dateFilter.reset")}
                  </button>
                  <button
                    onClick={closeDatePopup}
                    className="text-xs px-2.5 py-1 bg-sky-600 text-white rounded-md hover:bg-sky-700 transition-colors border-0 cursor-pointer"
                  >
                    {t("common.close")}
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Two-layer stack */}
        <div
          style={{
            display: "grid",
            gridTemplate: '"stack" / 1fr',
            minHeight: "60vh",
          }}
        >
          {/* Cached layer */}
          <div
            style={{
              gridArea: "stack",
              opacity: isSearchMode ? 0 : 1,
              pointerEvents: isSearchMode ? "none" : "auto",
              transition: "opacity 180ms ease-out",
            }}
          >
            <AgendaSections
              groups={cachedGrouped}
              onOpen={(id) => navigate(`/notes/${id}`)}
              emptyMessage={t("notes.empty.filtered")}
            />
            {hasMoreCached && (
              <div className="mt-7 text-center">
                <button
                  onClick={() => {
                    const next = page + 1;
                    setPage(next);
                    fetchCached(next, true, dateFrom, dateTo);
                  }}
                  className="inline-flex items-center gap-1.5 text-sm text-gray-500 hover:text-gray-900 transition-colors bg-transparent border-0 cursor-pointer px-4 py-2 rounded-full hover:bg-white/60"
                >
                  <ChevronDown className="w-3.5 h-3.5" />
                  <span>{t("notes.more")}</span>
                </button>
              </div>
            )}
          </div>

          {/* Search layer */}
          <div
            style={{
              gridArea: "stack",
              opacity: isSearchMode ? 1 : 0,
              pointerEvents: isSearchMode ? "auto" : "none",
              transition: "opacity 180ms ease-out",
            }}
          >
            {searchSettled && (
              <>
                <AgendaSections
                  groups={searchGrouped}
                  onOpen={(id) => navigate(`/notes/${id}`)}
                  emptyMessage={t("notes.search.empty")}
                />
                {hasMoreSearch && (
                  <div className="mt-7 text-center">
                    <button
                      onClick={() => {
                        const next = searchPage + 1;
                        setSearchPage(next);
                        fetchSearch(next, true, debouncedQuery, searchTags, dateFrom, dateTo);
                      }}
                      className="inline-flex items-center gap-1.5 text-sm text-gray-500 hover:text-gray-900 transition-colors bg-transparent border-0 cursor-pointer px-4 py-2 rounded-full hover:bg-white/60"
                    >
                      <ChevronDown className="w-3.5 h-3.5" />
                      <span>{t("notes.more")}</span>
                    </button>
                  </div>
                )}
              </>
            )}
          </div>
        </div>
      </div>
      {typePickerOpen && (
        <NewNoteTypeModal
          onPick={createWithType}
          onClose={() => setTypePickerOpen(false)}
          disabled={creating}
        />
      )}
    </div>
  );
}

/** 새 노트 유형 선택 모달 — 고르기 전엔 노트를 만들지 않는다(뒤로=닫기). */
function NewNoteTypeModal({
  onPick,
  onClose,
  disabled,
}: {
  onPick: (type: "freeform" | "minutes") => void;
  onClose: () => void;
  disabled: boolean;
}) {
  const { lang } = useLang();
  const en = lang === "en";
  const cards = [
    {
      type: "freeform" as const,
      name: en ? "Freeform note" : "노트 필기형",
      desc: en ? "AI writes as you chat" : "채팅하면 AI가 받아적어요",
      Icon: PenLine,
      color: "text-sky-400",
    },
    {
      type: "minutes" as const,
      name: en ? "Minutes" : "회의록 작성형",
      desc: en ? "Record → transcribe → organize" : "녹음 → 전사 → 정리",
      Icon: ClipboardList,
      color: "text-indigo-400",
    },
  ];
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 p-6"
      onClick={onClose}
    >
      <div className="relative bg-white rounded-2xl shadow-xl px-9 py-8" onClick={(e) => e.stopPropagation()}>
        <button
          type="button"
          onClick={onClose}
          disabled={disabled}
          className="absolute top-4 right-4 p-1 text-gray-400 hover:text-gray-700 rounded-md hover:bg-gray-100 bg-transparent border-0 cursor-pointer disabled:opacity-50"
          aria-label={en ? "Close" : "닫기"}
        >
          <X className="w-5 h-5" />
        </button>
        <h2 className="text-lg font-semibold text-gray-900 text-center">
          {en ? "Choose a note type" : "어떤 노트로 시작할까요?"}
        </h2>
        <div className="flex gap-6 mt-7">
          {cards.map((c) => (
            <button
              key={c.type}
              type="button"
              disabled={disabled}
              onClick={() => onPick(c.type)}
              className="group flex flex-col items-center gap-3 bg-transparent border-0 cursor-pointer disabled:opacity-50"
            >
              <div
                className="w-44 rounded-xl border border-gray-200 bg-gray-50 group-hover:border-sky-300 group-hover:bg-sky-50/40 transition-colors flex flex-col items-center justify-center gap-2"
                style={{ aspectRatio: "1 / 1.414" }}
              >
                <c.Icon className={`w-9 h-9 ${c.color}`} strokeWidth={1.5} />
                <span className="text-[11px] text-gray-400 px-2 text-center">{c.desc}</span>
              </div>
              <span className="text-sm font-medium text-gray-800">{c.name}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

function AgendaSections({
  groups,
  onOpen,
  emptyMessage,
}: {
  groups: AgendaGroup[];
  onOpen: (id: string) => void;
  emptyMessage: string;
}) {
  if (groups.length === 0) {
    return (
      <div className="py-16 text-center">
        <p className="text-sm text-gray-400">{emptyMessage}</p>
      </div>
    );
  }
  return (
    <>
      {groups.map((g, gi) => (
        <section key={g.label + (g.sub ?? "")} className={gi === 0 ? "" : "mt-7"}>
          <div className="flex items-baseline gap-2.5 mb-2 px-1">
            <h2 className="text-[11px] font-semibold uppercase tracking-[0.08em] text-gray-500">
              {g.label}
            </h2>
            {g.sub && <span className="text-[11px] text-gray-400">{g.sub}</span>}
          </div>
          <div className="space-y-1.5">
            {g.items.map((n) => (
              <AgendaRow key={n.id} note={n} onClick={() => onOpen(n.id)} />
            ))}
          </div>
        </section>
      ))}
    </>
  );
}

function AgendaRow({ note, onClick }: { note: NoteListItem; onClick: () => void }) {
  const { t, lang } = useLang();
  const active = note.has_active_task > 0;
  const status = active
    ? { dot: "bg-amber-500", label: t("notes.status.processing"), color: "text-amber-700" }
    : { dot: "bg-sky-500", label: t("notes.status.ready"), color: "text-gray-500" };
  return (
    <div
      onClick={onClick}
      className="group flex items-start gap-4 px-4 py-3.5 rounded-xl border bg-white border-gray-100 shadow-sm hover:shadow hover:-translate-y-px transition-[transform,box-shadow] duration-150 cursor-pointer transform-gpu will-change-transform"
    >
      <div className="w-16 shrink-0 pt-0.5">
        <div className="text-sm font-medium tabular-nums text-gray-900 leading-none">
          {formatTime(note.started_at)}
        </div>
      </div>
      <div className="flex-1 min-w-0">
        <h3 className="flex items-center gap-1.5 text-[15px] font-medium text-gray-900 leading-snug mb-0.5">
          {note.note_type === "minutes" ? (
            <Mic size={14} className="text-amber-500 shrink-0" />
          ) : (
            <FileText size={14} className="text-sky-500 shrink-0" />
          )}
          <span className="truncate">{note.title}</span>
        </h3>
        {note.description && (
          <p className="text-sm text-gray-500 line-clamp-1 leading-snug">{note.description}</p>
        )}
        {note.location && (
          <div className="flex items-center gap-2 text-[11px] text-gray-400 mt-1.5">
            <span className="inline-flex items-center gap-1">
              <MapPin className="w-2.5 h-2.5" />
              {note.location}
            </span>
          </div>
        )}
        {note.tags && note.tags.length > 0 && (
          <div className="flex flex-wrap items-center gap-2 mt-2.5">
            {note.tags.map((tg) => (
              <span
                key={tg.id}
                className="text-[12px] text-sky-600 leading-none"
              >
                #{tg.name}
              </span>
            ))}
          </div>
        )}
      </div>
      <div className="flex items-center gap-1.5 shrink-0 pt-1">
        <span className={`w-1.5 h-1.5 rounded-full ${status.dot}`} />
        <span className={`text-xs ${status.color}`}>{status.label}</span>
      </div>
      <ArrowRight className="w-3.5 h-3.5 text-gray-300 group-hover:text-gray-600 transition-colors shrink-0 mt-1.5" />
    </div>
  );
}
