import { useEffect, useMemo, useState } from "react";
import { Clock, X, RotateCcw } from "lucide-react";
import { toast } from "sonner";

import { processingApi, NoteBody } from "@/api/processing";
import Spinner from "@/components/Spinner";
import { useT } from "@/i18n/LangContext";

// Version history / restore modal — ported from the old MinutesVersionHistory.
// Left rail lists completed versions (newest first); right shows the selected
// version in a sandboxed iframe; footer restores a non-current version.

function formatDate(iso: string): string {
  const d = new Date(iso);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}.${pad(d.getMonth() + 1)}.${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export default function VersionHistory({
  noteId,
  initialVersionId,
  onClose,
  onRestored,
}: {
  noteId: string;
  /** Pre-select this version (e.g. from a chat "이 시점 본문 보기" chip). */
  initialVersionId?: string;
  onClose: () => void;
  onRestored: () => void;
}) {
  const t = useT();
  const [versions, setVersions] = useState<NoteBody[] | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [contentCache, setContentCache] = useState<Record<string, string | null>>({});
  const [restoring, setRestoring] = useState(false);
  const [iframeReady, setIframeReady] = useState(false);
  const [visible, setVisible] = useState(false);

  useEffect(() => setIframeReady(false), [selectedId]);
  useEffect(() => {
    const id = requestAnimationFrame(() => setVisible(true));
    return () => cancelAnimationFrame(id);
  }, []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  useEffect(() => {
    let cancelled = false;
    processingApi
      .listNoteBodies(noteId)
      .then((list) => {
        if (cancelled) return;
        const completed = list
          .filter((v) => v.status === "completed")
          .sort((a, b) => (a.created_at < b.created_at ? 1 : -1)); // newest first
        setVersions(completed);
        const initial =
          initialVersionId && completed.some((v) => v.id === initialVersionId)
            ? initialVersionId
            : (completed[0]?.id ?? null);
        setSelectedId(initial);
      })
      .catch(() => {
        if (!cancelled) toast.error(t("version.loadFail"));
      });
    return () => {
      cancelled = true;
    };
  }, [noteId]);

  useEffect(() => {
    if (!selectedId || selectedId in contentCache) return;
    let cancelled = false;
    processingApi
      .getBodyContent(selectedId)
      .then((c) => {
        if (!cancelled) setContentCache((prev) => ({ ...prev, [selectedId]: c }));
      })
      .catch(() => {
        if (!cancelled) setContentCache((prev) => ({ ...prev, [selectedId]: null }));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId, contentCache]);

  const current = useMemo(() => versions?.find((v) => v.id === selectedId) ?? null, [versions, selectedId]);
  const isCurrent = current ? current.archived === 0 : false;
  const contentLoaded = selectedId !== null && selectedId in contentCache;
  const content = contentLoaded ? contentCache[selectedId!] : undefined;

  const handleRestore = async () => {
    if (!current || isCurrent || restoring) return;
    setRestoring(true);
    try {
      await processingApi.restoreBody(noteId, current.id);
      toast.success(t("version.restored"));
      onRestored();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setRestoring(false);
    }
  };

  return (
    <div
      className={`fixed inset-0 bg-black/40 z-50 flex items-center justify-center p-6 backdrop-blur-[2px] transition-opacity duration-200 ${
        visible ? "opacity-100" : "opacity-0"
      }`}
      onClick={onClose}
    >
      <div
        className={`bg-white rounded-lg border border-[#dbdee3] shadow-xl flex flex-col w-full max-w-7xl h-[90vh] overflow-hidden transition-all duration-200 ease-out ${
          visible ? "opacity-100 scale-100" : "opacity-0 scale-95"
        }`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-4 px-5 py-3.5 border-b border-[#dbdee3] shrink-0">
          <div className="flex items-center gap-2">
            <Clock size={18} className="text-gray-700" />
            <span className="text-base font-medium text-gray-900">{t("version.title")}</span>
          </div>
          <div className="flex-1" />
          <button
            onClick={onClose}
            className="p-1.5 text-gray-500 hover:text-gray-900 hover:bg-gray-100 rounded-md bg-transparent border-0 cursor-pointer"
            title={t("common.close")}
          >
            <X size={18} />
          </button>
        </div>

        <div className="flex-1 min-h-0 grid grid-cols-12">
          <aside className="col-span-3 border-r border-gray-100 overflow-y-auto bg-gray-50/50">
            <div className="p-3 text-[11px] uppercase tracking-wider text-gray-400 font-medium">
              {t("version.list")} {versions ? `(${versions.length})` : ""}
            </div>
            {versions === null ? (
              <div className="px-3 pb-3 text-xs text-gray-400">{t("version.loading")}</div>
            ) : versions.length === 0 ? (
              <div className="px-3 pb-3 text-xs text-gray-400">{t("version.empty")}</div>
            ) : (
              <ul className="px-2 pb-3 space-y-1">
                {versions.map((v, idx) => {
                  const selected = v.id === selectedId;
                  const isActive = v.archived === 0;
                  const label = isActive ? t("version.current") : `v${versions.length - idx}`;
                  return (
                    <li key={v.id}>
                      <button
                        onClick={() => setSelectedId(v.id)}
                        className={`w-full text-left rounded-md px-3 py-2.5 border cursor-pointer transition-colors ${
                          selected
                            ? "bg-white border-sky-300 shadow-sm"
                            : "bg-transparent border-transparent hover:bg-white hover:border-gray-200"
                        }`}
                      >
                        <div className="flex items-center gap-2 mb-0.5">
                          <span className={`text-sm font-medium ${selected ? "text-sky-700" : "text-gray-900"}`}>
                            {formatDate(v.created_at)}
                          </span>
                          {isActive && (
                            <span className="text-[10px] uppercase tracking-wider text-sky-700 bg-sky-50 border border-sky-200 rounded px-1.5 py-0.5 ml-auto">
                              {t("version.current")}
                            </span>
                          )}
                        </div>
                        <div className="flex items-center gap-1.5 text-[11px] text-gray-400 tabular-nums font-mono">
                          <span>{label}</span>
                          {v.is_manual_edit === 1 && (
                            <span className="text-[10px] font-sans tracking-normal text-amber-700 bg-amber-50 border border-amber-200 rounded px-1.5 py-0.5">
                              {t("version.manual")}
                            </span>
                          )}
                        </div>
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
          </aside>

          <section className="col-span-9 flex flex-col min-h-0">
            <div className="flex-1 min-h-0 overflow-hidden">
              {!contentLoaded ? (
                <div className="flex flex-col items-center justify-center h-full gap-3">
                  <Spinner className="w-8 h-8" />
                  <p className="text-xs text-gray-400">{t("version.bodyLoading")}</p>
                </div>
              ) : content ? (
                <iframe
                  key={selectedId}
                  title={`body-${selectedId}`}
                  srcDoc={content}
                  sandbox=""
                  onLoad={() => setIframeReady(true)}
                  className={`w-full h-full border-0 transition-opacity duration-150 ${
                    iframeReady ? "opacity-100" : "opacity-0"
                  }`}
                />
              ) : (
                <div className="text-sm text-gray-400 px-6 py-5">{t("version.bodyEmpty")}</div>
              )}
            </div>

            <div className="flex items-center gap-3 px-5 py-3 border-t border-gray-100 shrink-0 bg-gray-50/50">
              <div className="text-xs text-gray-500 truncate min-w-0 flex-1">
                {isCurrent ? t("version.currentNote") : t("version.restoreNote")}
              </div>
              <button
                onClick={handleRestore}
                disabled={isCurrent || restoring || !current}
                className={`shrink-0 px-3 py-1.5 text-sm rounded-md border-0 cursor-pointer flex items-center gap-1.5 whitespace-nowrap ${
                  isCurrent || !current
                    ? "bg-gray-100 text-gray-400 cursor-not-allowed"
                    : restoring
                      ? "bg-sky-400 text-white cursor-wait"
                      : "bg-sky-600 text-white hover:bg-sky-700"
                }`}
              >
                <RotateCcw size={14} />
                {isCurrent ? t("version.currentBody") : restoring ? t("version.restoring") : t("version.restore")}
              </button>
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}
