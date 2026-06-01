import { useEffect, useRef, useState } from "react";
import { Maximize2, X, Copy, Check, Loader2 } from "lucide-react";

import { processingApi } from "@/api/processing";
import { useT } from "@/i18n/LangContext";

interface TranscriptModalProps {
  transcriptId: string;
  onClose: () => void;
}

function TranscriptModal({ transcriptId, onClose }: TranscriptModalProps) {
  const t = useT();
  const [content, setContent] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  // Drives the open-in animation. Starts false (opacity-0 + scale-95) on mount;
  // flipped true on the next frame so the transition runs from the initial
  // paint. Double rAF because React batches the state update with the mount
  // commit otherwise. (1f207ab)
  const [shown, setShown] = useState(false);
  const preRef = useRef<HTMLPreElement | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        // echo: Tauri command instead of the old HTTP GET /transcripts/{id}.
        const text = await processingApi.getTranscriptContent(transcriptId);
        if (cancelled) return;
        setContent(text);
      } catch {
        if (cancelled) return;
        setError(t("transcript.loadFail"));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [transcriptId]);

  useEffect(() => {
    const r1 = requestAnimationFrame(() => {
      const r2 = requestAnimationFrame(() => setShown(true));
      (r1 as unknown as { inner?: number }).inner = r2;
    });
    return () => {
      cancelAnimationFrame(r1);
      const r2 = (r1 as unknown as { inner?: number }).inner;
      if (r2 != null) cancelAnimationFrame(r2);
    };
  }, []);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  const handleCopy = async () => {
    if (!content) return;
    try {
      await navigator.clipboard.writeText(content);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // navigator.clipboard may be unavailable in some contexts.
    }
  };

  return (
    <div
      className={`fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm p-4 transition-opacity duration-200 ease-out ${
        shown ? "opacity-100" : "opacity-0"
      }`}
      onClick={onClose}
    >
      <div
        className={`w-full max-w-3xl max-h-[85vh] flex flex-col bg-white rounded-xl shadow-2xl border border-gray-200 transform transition-all duration-200 ease-out ${
          shown ? "opacity-100 scale-100 translate-y-0" : "opacity-0 scale-95 translate-y-2"
        }`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-5 py-3 border-b border-gray-100">
          <h2 className="text-sm font-semibold text-gray-900">{t("transcript.title")}</h2>
          <div className="flex items-center gap-1">
            <button
              type="button"
              onClick={handleCopy}
              title={copied ? t("md.copied") : t("md.copy")}
              aria-label={copied ? t("md.copied") : t("md.copy")}
              className="p-1.5 rounded-md text-gray-500 hover:text-gray-900 hover:bg-gray-100 transition-colors cursor-pointer bg-transparent border-0"
            >
              {copied ? <Check size={16} className="text-sky-600" /> : <Copy size={16} />}
            </button>
            <button
              type="button"
              onClick={onClose}
              title={t("common.close")}
              aria-label={t("common.close")}
              className="p-1.5 rounded-md text-gray-500 hover:text-gray-900 hover:bg-gray-100 transition-colors cursor-pointer bg-transparent border-0"
            >
              <X size={16} />
            </button>
          </div>
        </div>
        <div className="flex-1 overflow-auto p-5">
          {loading && (
            <div className="flex items-center justify-center gap-2 py-12 text-sm text-gray-500">
              <Loader2 size={16} className="animate-spin" />
              <span>{t("transcript.loading")}</span>
            </div>
          )}
          {!loading && error && <div className="py-12 text-center text-sm text-red-600">{error}</div>}
          {!loading && !error && (
            <pre
              ref={preRef}
              className="p-4 rounded-md bg-gray-50 border border-gray-200 text-[13px] font-mono leading-relaxed text-gray-800 whitespace-pre-wrap"
            >
              {content || t("transcript.empty")}
            </pre>
          )}
        </div>
      </div>
    </div>
  );
}

interface TranscriptBlockProps {
  transcriptId: string;
  previewText: string;
}

// Rendered in place of a normal fenced code block when the chat agent emits
// ```transcript-<uuid> ... ``` from the read_transcript tool. Shows a short
// preview inline and opens TranscriptModal for the full text on demand — the
// modal fetches by transcript_id lazily so the chat message itself stays small
// (and the LLM context on the next turn stays cheap). (1f207ab)
export function TranscriptBlock({ transcriptId, previewText }: TranscriptBlockProps) {
  const t = useT();
  const [open, setOpen] = useState(false);
  return (
    <>
      <div className="relative my-2 group">
        <pre className="p-3 pr-24 rounded-md bg-gray-50 border border-gray-200 overflow-x-auto text-[12px] font-mono leading-relaxed text-gray-800 whitespace-pre-wrap max-h-60">
          {previewText}
        </pre>
        <button
          type="button"
          onClick={() => setOpen(true)}
          title={t("transcript.viewFull")}
          aria-label={t("transcript.viewFull")}
          className="absolute top-1.5 right-1.5 inline-flex items-center gap-1 px-2 py-1 rounded-md bg-white/95 border border-gray-200 text-[11px] text-gray-700 hover:text-gray-900 hover:bg-white shadow-sm transition-colors cursor-pointer"
        >
          <Maximize2 size={12} />
          <span>{t("transcript.viewFullShort")}</span>
        </button>
      </div>
      {open && <TranscriptModal transcriptId={transcriptId} onClose={() => setOpen(false)} />}
    </>
  );
}

// Extract a transcript UUID from a react-markdown code-fence className like
// "language-transcript-<uuid>". Returns null when it doesn't match — caller
// falls back to the normal CodeBlock renderer. (1f207ab)
export function matchTranscriptClassName(className: string | undefined): string | null {
  if (!className) return null;
  const m = /^language-transcript-([0-9a-fA-F-]{8,})$/.exec(className);
  return m ? m[1] : null;
}
