import { useEffect, useState, useCallback, useRef } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { FileText, Clock, Pencil, AlertTriangle, Check, RotateCcw, Copy, PenLine, ClipboardList, Palette } from "lucide-react";
import { toast } from "sonner";

import { notesApi, Note, UpdateNoteInput } from "@/api/notes";
import { recordingsApi, Recording } from "@/api/recordings";
import { processingApi, Transcript, NoteBody } from "@/api/processing";
import { deriveStage, Stage } from "@/lib/stage";
import { getSavedSource } from "@/lib/audioDevice";
import { useLang, useT, type TFunc } from "@/i18n/LangContext";
import type { DictKey } from "@/i18n/dict";
import { srcDocFor, isHtmlContent } from "@/lib/markdownDoc";
import { THEMES } from "@/lib/themes";
import { settingsApi } from "@/api/settings";
import SourceSelector from "@/components/SourceSelector";
import RecordingWidget from "@/components/RecordingWidget";
import Spinner from "@/components/Spinner";
import ChatPanel from "@/components/ChatPanel";
import VersionHistory from "@/components/VersionHistory";
import { TranscriptModal } from "@/components/TranscriptViewerModal";
import FileUploader from "@/components/FileUploader";

interface Progress {
  current: number;
  total: number;
  stage: string; // "prep" | "asr" | "post"
}

/**
 * Note detail — 4-stage state machine. Recording is native (cpal, D-023).
 * Phase 2 wires transcribe/generate: the page watches transcripts + note_bodies
 * (refreshed on the `note:updated` worker event) and shows live progress from
 * `transcribe:progress`.
 */
export default function NoteDetailPage() {
  const navigate = useNavigate();
  const { t } = useLang();
  const { id } = useParams<{ id: string }>();

  const [note, setNote] = useState<Note | null>(null);
  const [recordings, setRecordings] = useState<Recording[]>([]);
  const [transcripts, setTranscripts] = useState<Transcript[]>([]);
  const [noteBodies, setNoteBodies] = useState<NoteBody[]>([]);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [loading, setLoading] = useState(true);

  const [activeRecordingId, setActiveRecordingId] = useState<string | null>(null);
  const [elapsed, setElapsed] = useState(0);
  const elapsedIntervalRef = useRef<number | null>(null);

  const refresh = useCallback(async () => {
    if (!id) return;
    try {
      const [n, recs, ts, bodies] = await Promise.all([
        notesApi.get(id),
        recordingsApi.listForNote(id),
        processingApi.listTranscripts(id),
        processingApi.listNoteBodies(id),
      ]);
      setNote(n);
      setRecordings(recs);
      setTranscripts(ts);
      setNoteBodies(bodies);
    } catch (e) {
      toast.error(String(e));
    }
  }, [id]);

  useEffect(() => {
    setLoading(true);
    refresh().finally(() => setLoading(false));
  }, [refresh]);

  // Worker events: live progress + completion → refresh.
  useEffect(() => {
    const unlistenProg = listen<{ note_id: string } & Progress>("transcribe:progress", (e) => {
      if (e.payload.note_id !== id) return;
      setProgress({ current: e.payload.current, total: e.payload.total, stage: e.payload.stage });
    });
    const unlistenUpd = listen("note:updated", () => {
      void refresh();
    });
    // App may be tray-minimized while transcribe/generate finishes; webview JS
    // throttles in the background, so the note:updated refresh can lag and we'd
    // briefly show a stale transcribing monitor on return. Re-sync the moment
    // the window becomes visible again (e.g. via the completion notification).
    const onVisible = () => {
      if (document.visibilityState === "visible") void refresh();
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      unlistenProg.then((fn) => fn());
      unlistenUpd.then((fn) => fn());
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [id, refresh]);

  const baseStage: Stage = deriveStage({ recordings, transcripts, noteBodies });
  // Progress events arrive live, but the derived stage only refreshes on the
  // note:updated event — which lags a whole transcribe run behind. Treat an
  // in-flight progress signal as transcribing so the monitor shows from the
  // prep step instead of a bare spinner until generate.
  const stage: Stage = progress && baseStage !== "done" ? "transcribing" : baseStage;

  const anyProcessing =
    transcripts.some((t) => t.status === "pending" || t.status === "processing") ||
    noteBodies.some((b) => b.status === "pending" || b.status === "processing");
  const anyFailed =
    transcripts.some((t) => ["failed", "empty", "cancelled"].includes(t.status)) ||
    noteBodies.some((b) => ["failed", "cancelled"].includes(b.status));
  // Failure kind drives the recovery panel (original design). transcript
  // failure vs minutes(body) failure get different copy.
  const failureKind: "transcript" | "minutes" | null = transcripts.some(
    (t) => t.status === "failed" || t.status === "cancelled",
  )
    ? "transcript"
    : noteBodies.some((b) => b.status === "failed" || b.status === "cancelled")
      ? "minutes"
      : null;
  // In the transcribing stage but nothing running AND not a failure → a
  // finalized recording whose transcription was never kicked off → offer start.
  // never-started recovery: a finalized recording with no transcript and no
  // live progress signal. `!progress` keeps the optimistic prep window (set on
  // stop) showing the monitor rather than this start card.
  const showRetry =
    baseStage === "transcribing" && !anyProcessing && failureKind === null && !progress;
  // Must filter archived: after a refine, the old body stays completed+archived,
  // so status alone would surface the *previous* version in the done panel.
  const activeBody = noteBodies.find((b) => b.archived === 0 && b.status === "completed") ?? null;

  // Drop the live progress once the body is done → DonePanel takes over.
  useEffect(() => {
    if (baseStage === "done") setProgress(null);
  }, [baseStage]);

  const startElapsed = () => {
    setElapsed(0);
    const start = Date.now();
    elapsedIntervalRef.current = window.setInterval(() => {
      setElapsed(Math.floor((Date.now() - start) / 1000));
    }, 250);
  };
  const stopElapsed = () => {
    if (elapsedIntervalRef.current) {
      clearInterval(elapsedIntervalRef.current);
      elapsedIntervalRef.current = null;
    }
  };

  const handleStart = useCallback(async () => {
    if (!id) return;
    const src = getSavedSource();
    if (!src) {
      toast.error(t("detail.toast.selectSource"));
      return;
    }
    try {
      const rec = await recordingsApi.start(id, src.name, src.source);
      setActiveRecordingId(rec.id);
      startElapsed();
      await refresh();
    } catch (e) {
      toast.error(t("detail.toast.recordStartFail", { error: String(e) }));
    }
  }, [id, refresh]);

  const handleStop = useCallback(async () => {
    stopElapsed();
    // finalize + transcribe prep run before the first backend progress event —
    // show the transcribing monitor immediately at the first (finalize) step.
    setProgress({ current: 0, total: 1, stage: "finalize" });
    const recId = activeRecordingId;
    setActiveRecordingId(null);
    if (recId) {
      try {
        await recordingsApi.stop(recId);
      } catch (e) {
        toast.error(t("detail.toast.recordStopFail", { error: String(e) }));
      }
    }
    setTimeout(refresh, 300);
  }, [activeRecordingId, refresh]);

  const onRetry = useCallback(async () => {
    if (!id) return;
    setProgress(null);
    try {
      await processingApi.retryTranscribe(id);
      toast.success(t("detail.toast.retranscribe"));
      await refresh();
    } catch (e) {
      toast.error(String(e));
    }
  }, [id, refresh]);

  // F-REC-004 — import an external audio file; backend converts + auto-runs the
  // transcribe chain, so a refresh flips the stage to transcribing.
  const handleImport = useCallback(
    async (srcPath: string) => {
      if (!id) return;
      try {
        await recordingsApi.importAudioFile(id, srcPath);
        await refresh();
      } catch (e) {
        toast.error(t("detail.toast.importFail", { error: String(e) }));
      }
    },
    [id, refresh],
  );

  useEffect(() => () => stopElapsed(), []);

  const onPatchMeta = useCallback(
    async (input: UpdateNoteInput) => {
      if (!id) return;
      try {
        const updated = await notesApi.update(id, input);
        setNote(updated);
      } catch (e) {
        toast.error(String(e));
      }
    },
    [id],
  );

  const onDelete = async () => {
    if (!id) return;
    if (!confirm(t("detail.delete.confirm"))) return;
    try {
      await notesApi.delete(id);
      toast.success(t("detail.toast.deleted"));
      navigate("/notes");
    } catch (e) {
      toast.error(String(e));
    }
  };

  if (loading) {
    return (
      <div className="min-h-screen flex items-center justify-center">
        <Spinner className="w-6 h-6" />
      </div>
    );
  }
  if (!note) {
    return <div className="p-6 text-sm text-gray-500">{t("detail.notFound")}</div>;
  }
  // 유형 미선택 → 선택 화면. 선택 후 고정(바꾸려면 새 노트).
  if (note.note_type == null) {
    return <NoteTypePicker noteId={id!} onPicked={refresh} />;
  }
  // 노트 필기형 — 채팅 우선: 좌 ChatPanel + 우 노트 본문(라이브).
  if (note.note_type === "freeform") {
    return <FreeformPage noteId={id!} note={note} refreshNote={refresh} />;
  }

  return (
    <div className="h-screen flex flex-col overflow-hidden">
      <div
        className="flex-1 grid min-h-0"
        style={{ gridTemplateColumns: "26.5rem 1fr", gridTemplateRows: "minmax(0, 1fr)", minHeight: 0 }}
      >
        <ChatPanel
          noteId={id!}
          note={note}
          stage={stage}
          elapsed={elapsed}
          progressPct={
            progress
              ? computeTranscribingProgress(progress.stage as TStep, {
                  current: progress.current,
                  total: progress.total,
                })
              : 0
          }
          transcribingStep={progress?.stage}
          failureKind={failureKind}
          capability={{
            hasFinalizedRecording: recordings.some((r) => r.format === "webm"),
            hasActiveBody: !!activeBody,
            hasFailedTranscript: transcripts.some(
              (t) => t.status === "failed" || t.status === "cancelled",
            ),
            hasFailedBody: noteBodies.some(
              (b) => b.archived === 0 && (b.status === "failed" || b.status === "cancelled"),
            ),
            archivedBodyCount: noteBodies.filter(
              (b) => b.archived !== 0 && b.status === "completed",
            ).length,
          }}
          onBack={() => navigate("/notes")}
          onDelete={onDelete}
          onRetry={onRetry}
          onAfterTurn={refresh}
        />
        <StagePanel
          stage={stage}
          note={note}
          activeRecordingId={activeRecordingId}
          elapsed={elapsed}
          progress={progress}
          showRetry={showRetry}
          failureKind={failureKind}
          activeBody={activeBody}
          onStart={handleStart}
          onStop={handleStop}
          onRetry={onRetry}
          onRestored={refresh}
          onPatchMeta={onPatchMeta}
          onImport={handleImport}
        />
      </div>
    </div>
  );
}

// ============================================================================

/** 새 노트 진입 시 유형 선택 (회의록 작성형 / 노트 필기형). 선택 후 고정. */
function NoteTypePicker({ noteId, onPicked }: { noteId: string; onPicked: () => void | Promise<void> }) {
  const { lang } = useLang();
  const en = lang === "en";
  const [busy, setBusy] = useState(false);
  const pick = async (type: "freeform" | "minutes") => {
    if (busy) return;
    setBusy(true);
    try {
      // 노트 스타일(테마)은 freeform 전용 — 설정(default_theme_freeform) 값, 없으면
      // 노란 공책. 회의록 작성형은 항상 고정 기본 테마.
      let theme = "default";
      if (type === "freeform") {
        const saved = await settingsApi.get("default_theme_freeform").catch(() => null);
        theme = saved && THEMES.some((th) => th.id === saved) ? saved : "notepad";
      }
      await notesApi.update(noteId, { note_type: type, theme });
      await onPicked();
    } catch (e) {
      toast.error(String(e));
      setBusy(false);
    }
  };
  const cards = [
    {
      type: "freeform" as const,
      name: en ? "Freeform note" : "노트 필기형",
      desc: en ? "Write by chatting; AI fills the note" : "채팅하며 AI가 노트를 채워줘요",
      icon: <PenLine className="w-10 h-10 text-sky-400" strokeWidth={1.5} />,
    },
    {
      type: "minutes" as const,
      name: en ? "Minutes" : "회의록 작성형",
      desc: en ? "Record → transcribe → organize" : "녹음 → 전사 → 정리",
      icon: <ClipboardList className="w-10 h-10 text-indigo-400" strokeWidth={1.5} />,
    },
  ];
  return (
    <div className="min-h-screen flex flex-col items-center justify-center gap-10 bg-white px-6">
      <div className="text-center">
        <h1 className="text-xl font-semibold text-gray-900">
          {en ? "Choose a note type" : "어떤 노트로 시작할까요?"}
        </h1>
      </div>
      <div className="flex gap-8">
        {cards.map((c) => (
          <button
            key={c.type}
            type="button"
            onClick={() => pick(c.type)}
            disabled={busy}
            className="group flex flex-col items-center gap-3 bg-transparent border-0 cursor-pointer disabled:opacity-50"
          >
            <div
              className="w-52 rounded-xl border border-gray-200 bg-gray-50 group-hover:border-sky-300 group-hover:bg-sky-50/40 transition-colors flex flex-col items-center justify-center gap-2"
              style={{ aspectRatio: "1 / 1.414" }}
            >
              {c.icon}
              <span className="text-[11px] text-gray-400">{c.desc}</span>
            </div>
            <span className="text-sm font-medium text-gray-800">{c.name}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

/**
 * 노트 필기형 페이지 — 좌측 ChatPanel(stage="freeform") + 우측 노트 본문.
 * 빈 노트로 시작해 채팅하면 write_note가 본문을 채우고, onAfterTurn으로 갱신된다.
 */
function FreeformPage({
  noteId,
  note,
  refreshNote,
}: {
  noteId: string;
  note: Note;
  refreshNote: () => void | Promise<void>;
}) {
  const { t } = useLang();
  const navigate = useNavigate();
  const [bodies, setBodies] = useState<NoteBody[]>([]);
  const refreshBodies = useCallback(() => {
    processingApi.listNoteBodies(noteId).then(setBodies).catch(() => {});
  }, [noteId]);
  useEffect(() => {
    refreshBodies();
  }, [refreshBodies]);
  const activeBody = bodies.find((b) => b.archived === 0 && b.status === "completed") ?? null;

  const onDelete = async () => {
    if (!confirm(t("detail.delete.confirm"))) return;
    try {
      await notesApi.delete(noteId);
      navigate("/notes");
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="h-screen flex flex-col overflow-hidden">
      <div
        className="flex-1 grid min-h-0"
        style={{ gridTemplateColumns: "26.5rem 1fr", gridTemplateRows: "minmax(0, 1fr)", minHeight: 0 }}
      >
        <ChatPanel
          noteId={noteId}
          note={note}
          stage="freeform"
          capability={{
            hasFinalizedRecording: false,
            hasActiveBody: !!activeBody,
            hasFailedTranscript: false,
            hasFailedBody: false,
            archivedBodyCount: bodies.filter((b) => b.archived !== 0 && b.status === "completed")
              .length,
          }}
          onBack={() => navigate("/notes")}
          onDelete={onDelete}
          onRetry={() => {}}
          onAfterTurn={() => {
            refreshBodies();
            refreshNote();
          }}
        />
        <div className="h-full min-h-0 p-6 flex flex-col">
          {/* 빈/본문 모두 같은 셸(DonePanel). body=null이면 빈 노란 패드 + 헤더 버튼. */}
          <DonePanel body={activeBody} note={note} onRestored={refreshBodies} variant="freeform" />
        </div>
      </div>
    </div>
  );
}

// ============================================================================

function StagePanel(props: {
  stage: Stage;
  note: Note;
  activeRecordingId: string | null;
  elapsed: number;
  progress: Progress | null;
  showRetry: boolean;
  failureKind: "transcript" | "minutes" | null;
  activeBody: NoteBody | null;
  onStart: () => Promise<void>;
  onStop: () => void;
  onRetry: () => void;
  onRestored: () => void;
  onPatchMeta: (input: UpdateNoteInput) => Promise<void>;
  onImport: (srcPath: string) => Promise<void>;
}) {
  switch (props.stage) {
    case "before":
      return (
        <BeforePanel
          note={props.note}
          onStart={props.onStart}
          onPatchMeta={props.onPatchMeta}
          onImport={props.onImport}
        />
      );
    case "recording":
      return props.activeRecordingId ? (
        <RecordingWidget
          recordingId={props.activeRecordingId}
          elapsed={props.elapsed}
          onStop={props.onStop}
        />
      ) : (
        <section className="flex items-center justify-center p-12">
          <Spinner className="w-6 h-6" />
        </section>
      );
    case "transcribing":
      return (
        <TranscribingPanel
          progress={props.progress}
          showRetry={props.showRetry}
          failureKind={props.failureKind}
          onRetry={props.onRetry}
        />
      );
    case "done":
      return (
        <div className="h-full min-h-0 p-6 flex flex-col">
          <DonePanel body={props.activeBody} note={props.note} onRestored={props.onRestored} />
        </div>
      );
  }
}

const LANG_OPTIONS: { v: string; labelKey: DictKey }[] = [
  { v: "auto", labelKey: "detail.lang.auto" },
  { v: "kor", labelKey: "detail.lang.kor" },
  { v: "eng", labelKey: "detail.lang.eng" },
];


function BeforePanel({
  note,
  onStart,
  onPatchMeta,
  onImport,
}: {
  note: Note;
  onStart: () => Promise<void>;
  onPatchMeta: (input: UpdateNoteInput) => Promise<void>;
  onImport: (srcPath: string) => Promise<void>;
}) {
  const t = useT();

  return (
    <section className="flex flex-col h-full min-h-0 items-center justify-center px-8 py-10 overflow-y-auto">
      <div className="w-full max-w-md flex flex-col items-center gap-7">
        <div className="text-center">
          <p className="text-sm uppercase tracking-wider text-gray-400 mb-2">Step 1</p>
          <h2 className="text-2xl font-medium text-gray-900">{t("detail.record.title")}</h2>
        </div>

        {/* Step 1 메타: 전사 언어만 선택. 제목·시간·장소는 본문 첫 줄/생성 시각으로 일원화(read-only). */}
        <div className="w-full flex flex-col items-center gap-2.5">
          <div className="inline-flex items-center rounded-full bg-gray-100 p-1">
            {LANG_OPTIONS.map((opt) => (
              <button
                key={opt.v}
                onClick={() => onPatchMeta({ language: opt.v })}
                className={`px-3 py-0.5 text-[11px] rounded-full cursor-pointer transition-colors border-0 ${
                  (note.language || "auto") === opt.v
                    ? "bg-white text-gray-900 shadow-sm"
                    : "bg-transparent text-gray-500 hover:text-gray-700"
                }`}
              >
                {t(opt.labelKey)}
              </button>
            ))}
          </div>
        </div>

        {/* Input source + test */}
        <div className="w-full">
          <SourceSelector />
        </div>

        <button
          onClick={onStart}
          className="px-6 py-2.5 bg-red-400 hover:bg-red-500 active:scale-95 rounded-full text-white shadow-md flex items-center gap-2 transition-all cursor-pointer border-0 text-sm font-medium"
        >
          <span className="w-2 h-2 bg-white rounded-full" />
          {t("detail.record.start")}
        </button>

        {/* F-REC-004 — file upload (drag-drop or click-to-pick). */}
        <div className="flex items-center gap-3 w-full">
          <div className="flex-1 h-px bg-gray-200" />
          <span className="text-xs text-gray-400">{t("detail.or")}</span>
          <div className="flex-1 h-px bg-gray-200" />
        </div>
        <div className="w-full">
          <FileUploader onSelect={onImport} />
        </div>
      </div>
    </section>
  );
}

// Pipeline steps in the transcribing monitor. Mirrors Meetzy's
// finalize/transcribe/minutes (08a1e89): the 3rd step is 회의록 생성
// (minutes / the generate worker), NOT post-processing. The LLM post-process
// lives inside the transcribe step with no progress of its own.
type TStep = "finalize" | "transcribe" | "minutes";

const T_STEPS: { id: TStep; labelKey: DictKey }[] = [
  { id: "finalize", labelKey: "detail.step.finalize" },
  { id: "transcribe", labelKey: "detail.step.transcribe" },
  { id: "minutes", labelKey: "detail.step.minutes" },
];
const T_STEP_INDEX: Record<TStep, number> = { finalize: 0, transcribe: 1, minutes: 2 };
const T_STEP_RANGES: Record<TStep, [number, number]> = { finalize: [0, 5], transcribe: [5, 95], minutes: [95, 100] };

// transcribe (the long step) gets the bulk and fills by chunk ratio, so the bar
// moves continuously instead of jumping between the three steps.
function computeTranscribingProgress(step: TStep, chunk: { current: number; total: number } | null): number {
  const range = T_STEP_RANGES[step];
  if (!range) return 0; // 알 수 없는/준비 스텝 — 모니터 진입 직후 크래시 방지
  const [lo, hi] = range;
  if (step === "transcribe") {
    if (chunk && chunk.total > 0) {
      return Math.round(lo + Math.min(1, chunk.current / chunk.total) * (hi - lo));
    }
    return lo;
  }
  return Math.round((lo + hi) / 2);
}

// Spinner heading — 단계별 문장형 문구 (a2d9a6f). echo는 개인앱이라 transcribe는
// "회의" 없이 기존 echo 문구를 유지한다.
const T_STEP_HEADING: Record<TStep, DictKey> = {
  finalize: "detail.heading.finalize",
  transcribe: "detail.heading.transcribe",
  minutes: "detail.heading.minutes",
};

function TranscribingPanel({
  progress,
  showRetry,
  failureKind,
  onRetry,
}: {
  progress: Progress | null;
  showRetry: boolean;
  failureKind: "transcript" | "minutes" | null;
  onRetry: () => void;
}) {
  const t = useT();
  // Failure — recovery panel (original design).
  if (failureKind) {
    const isMinutes = failureKind === "minutes";
    return (
      <div className="flex flex-col h-full min-h-0 items-center justify-center px-8 py-12">
        <div className="w-full max-w-md flex flex-col items-center gap-6">
          <div className="w-16 h-16 rounded-full bg-amber-50 flex items-center justify-center">
            <AlertTriangle className="w-8 h-8 text-amber-500" strokeWidth={1.75} />
          </div>
          <div className="text-center">
            <h2 className="text-xl font-medium text-gray-900">
              {isMinutes ? t("detail.fail.minutes.title") : t("detail.fail.transcribe.title")}
            </h2>
            <p className="text-sm text-gray-500 mt-2 leading-relaxed whitespace-pre-line">
              {isMinutes ? t("detail.fail.minutes.desc") : t("detail.fail.transcribe.desc")}
            </p>
          </div>
          <button
            onClick={onRetry}
            className="inline-flex items-center gap-1.5 px-4 py-2 bg-sky-600 text-white rounded-md text-sm font-medium hover:bg-sky-700 transition-colors border-0 cursor-pointer"
          >
            <RotateCcw size={14} />
            {t("detail.retry")}
          </button>
        </div>
      </div>
    );
  }

  // Finalized recording with transcription never started — offer to start.
  if (showRetry) {
    return (
      <div className="flex flex-col h-full min-h-0 items-center justify-center px-8 py-12">
        <div className="w-full max-w-md flex flex-col items-center gap-6">
          <div className="w-16 h-16 rounded-full bg-sky-50 flex items-center justify-center">
            <FileText className="w-8 h-8 text-sky-500" strokeWidth={1.75} />
          </div>
          <div className="text-center">
            <h2 className="text-xl font-medium text-gray-900">{t("detail.recordDone.title")}</h2>
            <p className="text-sm text-gray-500 mt-2">{t("detail.recordDone.desc")}</p>
          </div>
          <button
            onClick={onRetry}
            className="inline-flex items-center gap-1.5 px-4 py-2 bg-sky-600 text-white rounded-md text-sm font-medium hover:bg-sky-700 transition-colors border-0 cursor-pointer"
          >
            <RotateCcw size={14} />
            {t("detail.transcribe.start")}
          </button>
        </div>
      </div>
    );
  }

  // In progress — original 3-step monitor with the ring spinner.
  const step: TStep = (progress?.stage as TStep) ?? "finalize";
  const chunk = progress ? { current: progress.current, total: progress.total } : null;
  const currentIdx = T_STEP_INDEX[step] ?? 0;
  const pct = computeTranscribingProgress(step, chunk);

  return (
    <div className="flex flex-col h-full min-h-0 items-center justify-center px-8 py-12">
      <div className="w-full max-w-md flex flex-col items-center gap-10">
        <div className="text-center">
          <div className="w-16 h-16 mx-auto mb-4 rounded-full bg-sky-50 flex items-center justify-center">
            <Spinner className="w-10 h-10" />
          </div>
          <h2 className="text-xl font-medium text-gray-900">{t(T_STEP_HEADING[step])}</h2>
          <p className="text-sm text-gray-500 mt-2">{t("detail.transcribing.desc")}</p>
        </div>

        <div className="w-full space-y-2.5">
          <div className="flex items-center justify-between text-xs">
            <span className="text-gray-500">{t("detail.progress")}</span>
            <span className="font-medium text-sky-700">{pct}%</span>
          </div>
          <div className="w-full h-2 bg-gray-100 rounded-full overflow-hidden">
            <div className="h-full bg-sky-600 rounded-full transition-all duration-500" style={{ width: `${pct}%` }} />
          </div>
        </div>

        <div className="w-full space-y-3">
          {T_STEPS.map((s, idx) => {
            const done = idx < currentIdx;
            const active = idx === currentIdx;
            return (
              <div key={s.id} className="flex items-center gap-3 text-sm">
                <div
                  className={`w-5 h-5 rounded-full flex items-center justify-center shrink-0 ${
                    done ? "bg-sky-600 text-white" : active ? "bg-sky-50 border-2 border-sky-300" : "bg-gray-100"
                  }`}
                >
                  {done ? (
                    <Check size={12} />
                  ) : active ? (
                    <div className="w-1.5 h-1.5 bg-sky-600 rounded-full animate-pulse" />
                  ) : null}
                </div>
                <span className={done ? "text-gray-900" : active ? "text-gray-900 font-medium" : "text-gray-400"}>
                  {t(s.labelKey)}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

// 본문 디자인은 테마 프리셋(lib/themes.ts)이 담당한다 — 마크다운(신규)·HTML 조각
// (레거시 freeform)은 srcDocFor가 테마 CSS로 감싸고, 자체 <style>을 가진 레거시
// full-HTML minutes만 그대로 렌더한다. 기존 노란 괘선 템플릿은 notepad 테마로 흡수.

function DonePanel({
  body,
  note,
  onRestored,
  variant = "minutes",
}: {
  body: NoteBody | null;
  note: Note;
  onRestored: () => void;
  variant?: "minutes" | "freeform";
}) {
  const t = useT();
  const { lang } = useLang();
  const noteId = note.id;
  const [html, setHtml] = useState<string | null>(null);
  const [loadingBody, setLoadingBody] = useState(true);
  const [showVersions, setShowVersions] = useState(false);
  const [showTranscript, setShowTranscript] = useState(false);
  const [transcriptId, setTranscriptId] = useState<string | null>(null);
  const [isEditing, setIsEditing] = useState(false);
  const [editContent, setEditContent] = useState("");
  const [saving, setSaving] = useState(false);
  const [copiedType, setCopiedType] = useState<string | null>(null);
  const [showCopyMenu, setShowCopyMenu] = useState(false);
  const [theme, setTheme] = useState(note.theme || "default");
  const [showThemeMenu, setShowThemeMenu] = useState(false);
  const editRef = useRef<HTMLIFrameElement | null>(null);
  const copyMenuRef = useRef<HTMLDivElement | null>(null);
  const themeMenuRef = useRef<HTMLDivElement | null>(null);

  // 레거시 HTML 본문(자체 스타일 보유/조각)이면 iframe-편집, 마크다운(신규)이면 raw
  // 텍스트 편집. 빈 본문(freeform 첫 작성)은 마크다운으로 시작한다.
  const htmlMode = !!html && isHtmlContent(html);

  useEffect(() => {
    setTheme(note.theme || "default");
  }, [note.theme]);

  useEffect(() => {
    let alive = true;
    setLoadingBody(true);
    setIsEditing(false);
    if (!body) {
      setHtml(null);
      setLoadingBody(false);
      return;
    }
    processingApi
      .getBodyContent(body.id)
      .then((c) => {
        if (alive) setHtml(c);
      })
      .catch(() => {
        if (alive) setHtml(null);
      })
      .finally(() => {
        if (alive) setLoadingBody(false);
      });
    return () => {
      alive = false;
    };
  }, [body]);

  // minutes: load this note's completed transcript id so the 전사록 button can
  // open the full-text modal. Reloads when the body changes (after generate).
  useEffect(() => {
    if (variant !== "minutes") return;
    let alive = true;
    processingApi
      .listTranscripts(noteId)
      .then((ts) => {
        if (!alive) return;
        const done = ts.find((x) => x.status === "completed");
        setTranscriptId(done ? done.id : null);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [noteId, variant, body]);

  // Close the copy/theme menus on outside click. iframe clicks don't bubble to
  // the parent document, so also close on window blur (clicking into the body
  // iframe blurs the parent window) to catch clicks landing on the content.
  useEffect(() => {
    if (!showCopyMenu && !showThemeMenu) return;
    function onDoc(e: MouseEvent) {
      if (copyMenuRef.current && !copyMenuRef.current.contains(e.target as Node)) setShowCopyMenu(false);
      if (themeMenuRef.current && !themeMenuRef.current.contains(e.target as Node)) setShowThemeMenu(false);
    }
    const onBlur = () => {
      setShowCopyMenu(false);
      setShowThemeMenu(false);
    };
    document.addEventListener("mousedown", onDoc);
    window.addEventListener("blur", onBlur);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      window.removeEventListener("blur", onBlur);
    };
  }, [showCopyMenu, showThemeMenu]);

  // 테마 선택 — 즉시 로컬 반영(뷰어 iframe 재렌더) + 서버 저장. 부모 refresh가
  // note를 다시 가져오면 useEffect가 동기화한다.
  const pickTheme = async (id: string) => {
    setTheme(id);
    setShowThemeMenu(false);
    try {
      await notesApi.update(noteId, { theme: id });
    } catch (e) {
      toast.error(String(e));
    }
  };

  // freeform은 본문이 없어도(빈 노트) 같은 셸을 그린다 — 헤더 버튼 일관 + 빈 줄 패드.
  if (!body && variant !== "freeform") return null;

  // Viewer iframe: size it to its content (scrolling="no") so the card body
  // scrolls as one piece, not an inner iframe scrollbar — matches MinutesView.
  const adjustIframeHeight = () => {
    const iframe = editRef.current;
    const doc = iframe?.contentDocument;
    if (iframe && doc?.body) {
      iframe.style.height = "0px";
      const h = Math.max(doc.body.scrollHeight, doc.documentElement.scrollHeight, doc.body.offsetHeight);
      iframe.style.height = `${h + 16}px`;
    }
  };

  // Make the edit iframe's body contentEditable once it loads (sandbox
  // allow-same-origin so we can read it back on save).
  const onEditLoad = () => {
    const doc = editRef.current?.contentDocument;
    if (doc?.body) {
      doc.body.contentEditable = "true";
      doc.body.style.outline = "none";
      doc.body.focus();
    }
  };
  const saveEdit = async () => {
    let edited: string;
    if (!htmlMode) {
      // 마크다운(신규) — textarea의 raw 마크다운을 그대로 저장.
      edited = editContent;
    } else {
      const doc = editRef.current?.contentDocument;
      if (!doc) return;
      // freeform 조각은 테마 CSS를 제외한 콘텐츠(body 안쪽)만 저장 → 렌더 때 다시 감쌈.
      edited =
        variant === "freeform"
          ? (doc.body?.innerHTML ?? "")
          : "<!DOCTYPE html>\n" + doc.documentElement.outerHTML;
    }
    setSaving(true);
    try {
      await processingApi.saveManualEdit(noteId, edited);
      toast.success(t("detail.toast.saved"));
      setIsEditing(false);
      onRestored();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSaving(false);
    }
  };

  // Copy — text(→markdown) / html(→inline-styled for Word/Docs). Ports MinutesView.
  const htmlToMarkdown = (el: HTMLElement): string => {
    const lines: string[] = [];
    for (const node of Array.from(el.childNodes)) {
      if (node.nodeType === Node.TEXT_NODE) {
        const t = node.textContent?.trim();
        if (t) lines.push(t);
        continue;
      }
      if (node.nodeType !== Node.ELEMENT_NODE) continue;
      const tag = (node as HTMLElement).tagName;
      const inner = (node as HTMLElement).innerText?.trim() || "";
      if (!inner) continue;
      if (tag === "H1") lines.push(`# ${inner}`, "");
      else if (tag === "H2") lines.push(`## ${inner}`, "");
      else if (tag === "H3") lines.push(`### ${inner}`, "");
      else if (tag === "UL") {
        for (const li of Array.from((node as HTMLElement).querySelectorAll(":scope > li")))
          lines.push(`- ${(li as HTMLElement).innerText?.trim()}`);
        lines.push("");
      } else if (tag === "OL") {
        Array.from((node as HTMLElement).querySelectorAll(":scope > li")).forEach((li, i) =>
          lines.push(`${i + 1}. ${(li as HTMLElement).innerText?.trim()}`),
        );
        lines.push("");
      } else if (tag === "P" || tag === "DIV") lines.push(inner, "");
      else if (tag === "TABLE") {
        for (const row of Array.from((node as HTMLElement).querySelectorAll("tr"))) {
          const cells = Array.from(row.querySelectorAll("th, td")).map((c) => (c as HTMLElement).innerText?.trim() || "");
          lines.push(`| ${cells.join(" | ")} |`);
        }
        lines.push("");
      } else lines.push(inner);
    }
    return lines.join("\n").replace(/\n{3,}/g, "\n\n").trim();
  };
  const handleCopyText = async () => {
    // 마크다운 본문은 원문 그대로가 곧 텍스트 사본. 레거시 HTML만 변환한다.
    const doc = editRef.current?.contentDocument;
    const text = !htmlMode ? html ?? "" : doc?.body ? htmlToMarkdown(doc.body) : html ?? "";
    await navigator.clipboard.writeText(text);
    setCopiedType("text");
    setShowCopyMenu(false);
    setTimeout(() => setCopiedType(null), 2000);
  };
  const handleCopyHtml = async () => {
    const iframe = editRef.current;
    const doc = iframe?.contentDocument;
    const textFallback = doc?.body ? doc.body.innerText : html ?? "";
    let bodyHtml = doc?.body ? doc.body.innerHTML : html ?? "";
    if (doc?.body && iframe?.contentWindow) {
      const win = iframe.contentWindow;
      const tableStyles = Array.from(doc.body.querySelectorAll("table")).map((el) => {
        const cs = win.getComputedStyle(el);
        return `width:${cs.width};border-collapse:${cs.borderCollapse};margin-bottom:${cs.marginBottom}`;
      });
      const cellStyles = Array.from(doc.body.querySelectorAll("th, td")).map((el) => {
        const cs = win.getComputedStyle(el);
        return `border:1px solid ${cs.borderColor};padding:${cs.padding};text-align:${cs.textAlign};background-color:${cs.backgroundColor}`;
      });
      const headingStyles = Array.from(doc.body.querySelectorAll("h1, h2, h3")).map((el) => {
        const cs = win.getComputedStyle(el);
        return `color:${cs.color};background-color:${cs.backgroundColor};padding:${cs.padding};border-left:${cs.borderLeft};border-bottom:${cs.borderBottom};margin-top:${cs.marginTop}`;
      });
      const clone = doc.body.cloneNode(true) as HTMLElement;
      clone.querySelectorAll("table").forEach((el, i) => tableStyles[i] && el.setAttribute("style", tableStyles[i]));
      clone.querySelectorAll("th, td").forEach((el, i) => cellStyles[i] && el.setAttribute("style", cellStyles[i]));
      clone.querySelectorAll("h1, h2, h3").forEach((el, i) => headingStyles[i] && el.setAttribute("style", headingStyles[i]));
      bodyHtml = clone.innerHTML;
    }
    try {
      await navigator.clipboard.write([
        new ClipboardItem({
          "text/html": new Blob([bodyHtml], { type: "text/html" }),
          "text/plain": new Blob([textFallback], { type: "text/plain" }),
        }),
      ]);
    } catch {
      await navigator.clipboard.writeText(textFallback);
    }
    setCopiedType("html");
    setShowCopyMenu(false);
    setTimeout(() => setCopiedType(null), 2000);
  };


  return (
    <section
      className={`flex-1 min-h-0 flex flex-col rounded-lg shadow-md overflow-hidden w-full max-w-4xl mx-auto ${
        variant === "freeform" && theme === "notepad"
          ? "border border-amber-200/70"
          : "bg-white border border-[#dbdee3]"
      }`}
      style={variant === "freeform" && theme === "notepad" ? { backgroundColor: "#fffdf2" } : undefined}
    >
      <div
        className={`flex items-center justify-end gap-1 px-5 py-3 shrink-0 ${
          variant === "freeform" ? "" : "border-b border-gray-100"
        }`}
      >
        {isEditing ? (
          <>
            <button
              onClick={() => setIsEditing(false)}
              disabled={saving}
              className="px-2.5 py-1 text-xs text-gray-500 hover:text-gray-900 hover:bg-gray-100 rounded-md transition-colors bg-transparent border-0 cursor-pointer disabled:opacity-50"
            >
              {t("common.cancel")}
            </button>
            <button
              onClick={saveEdit}
              disabled={saving}
              className="px-3 py-1 bg-sky-600 text-white rounded-md text-xs font-medium hover:bg-sky-700 transition-colors border-0 cursor-pointer disabled:opacity-60"
            >
              {saving ? t("detail.saving") : t("detail.save")}
            </button>
          </>
        ) : (
          <>
            {/* 노트 스타일(테마) 전환 — freeform(노트 필기형) 전용. 회의록 작성형은
                고정 기본 테마(Meetzy 동일). */}
            {variant === "freeform" && (
            <div ref={themeMenuRef} className="relative">
              <button
                onClick={() => setShowThemeMenu((v) => !v)}
                className="inline-flex items-center gap-1.5 text-xs text-gray-500 hover:text-gray-900 hover:bg-gray-100 rounded-md px-2 py-1 bg-transparent border-0 cursor-pointer transition-colors"
                title={lang === "en" ? "Note style" : "노트 스타일"}
              >
                <Palette className="w-3.5 h-3.5" />
                {lang === "en" ? "Note style" : "노트 스타일"}
              </button>
              {showThemeMenu && (
                <div className="absolute right-0 top-full mt-1 z-20 bg-white border border-gray-300 rounded-lg shadow-lg py-1 w-36">
                  {THEMES.map((th) => (
                    <button
                      key={th.id}
                      onClick={() => pickTheme(th.id)}
                      className={`w-full text-left px-3 py-1.5 text-xs hover:bg-gray-50 bg-transparent border-0 cursor-pointer flex items-center gap-2 ${
                        theme === th.id ? "text-sky-700 font-medium" : "text-gray-700"
                      }`}
                    >
                      <span
                        className="w-3 h-3 rounded-full border border-gray-300 shrink-0"
                        style={{ backgroundColor: th.swatch }}
                      />
                      {lang === "en" ? th.nameEn : th.nameKo}
                      {theme === th.id && <Check className="w-3 h-3 ml-auto" />}
                    </button>
                  ))}
                </div>
              )}
            </div>
            )}
            {variant === "minutes" && transcriptId && (
              <button
                onClick={() => setShowTranscript(true)}
                className="inline-flex items-center gap-1.5 text-xs text-gray-500 hover:text-gray-900 hover:bg-gray-100 rounded-md px-2 py-1 bg-transparent border-0 cursor-pointer transition-colors"
                title={t("detail.transcript")}
              >
                <FileText className="w-3.5 h-3.5" />
                {t("detail.transcript")}
              </button>
            )}
            <button
              onClick={() => setShowVersions(true)}
              className="inline-flex items-center gap-1.5 text-xs text-gray-500 hover:text-gray-900 hover:bg-gray-100 rounded-md px-2 py-1 bg-transparent border-0 cursor-pointer transition-colors"
              title={t("detail.history")}
            >
              <Clock className="w-3.5 h-3.5" />
              {t("detail.history")}
            </button>
            <button
              onClick={() => {
                setEditContent(html ?? "");
                setIsEditing(true);
              }}
              disabled={!html && variant !== "freeform"}
              className="inline-flex items-center gap-1.5 text-xs text-gray-500 hover:text-gray-900 hover:bg-gray-100 rounded-md px-2 py-1 bg-transparent border-0 cursor-pointer transition-colors disabled:opacity-40"
              title={t("detail.editManual")}
            >
              <Pencil className="w-3.5 h-3.5" />
              {t("detail.edit")}
            </button>
          </>
        )}
      </div>
      <div className={`flex-1 min-h-0 ${variant === "freeform" ? "flex flex-col" : "p-6 overflow-y-auto"}`}>
        {loadingBody ? (
          <div className="flex items-center justify-center h-full">
            <Spinner className="w-6 h-6" />
          </div>
        ) : html || variant === "freeform" ? (
          <div
            className={`${isEditing ? "relative h-full" : "relative"} ${
              variant === "freeform" ? "flex-1 min-h-0 flex flex-col" : ""
            }`}
          >
            {/* Copy overlay — text(→markdown) / html(→styled). Matches MinutesView. */}
            {!isEditing && (
              <div ref={copyMenuRef} className="absolute top-2 right-2 z-10">
                <button
                  onClick={() => setShowCopyMenu((v) => !v)}
                  className="p-1.5 bg-white border border-gray-300 rounded-md text-gray-500 hover:text-gray-700 hover:bg-gray-50 shadow-sm cursor-pointer"
                  title={t("detail.copy")}
                >
                  {copiedType ? <Check className="w-4 h-4 text-sky-600" /> : <Copy className="w-4 h-4" />}
                </button>
                {showCopyMenu && (
                  <div className="absolute right-0 top-full mt-1 bg-white border border-gray-300 rounded-lg shadow-lg py-1 w-36">
                    <button
                      onClick={handleCopyText}
                      className="w-full text-left px-3 py-1.5 text-xs text-gray-700 hover:bg-gray-50 bg-transparent border-0 cursor-pointer"
                    >
                      {t("detail.copy.text")}
                    </button>
                    <button
                      onClick={handleCopyHtml}
                      className="w-full text-left px-3 py-1.5 text-xs text-gray-700 hover:bg-gray-50 bg-transparent border-0 cursor-pointer"
                    >
                      {t("detail.copy.formatted")}
                    </button>
                  </div>
                )}
              </div>
            )}
            {isEditing && !htmlMode ? (
              // 마크다운(신규) — raw 마크다운을 textarea로 직접 편집 (Meetzy MinutesView
              // markdown edit mode와 동일). 빈 freeform 첫 작성도 이 경로.
              <textarea
                value={editContent}
                onChange={(e) => setEditContent(e.target.value)}
                className={`w-full resize-none px-4 py-3 border border-gray-200 rounded-md text-sm font-mono leading-relaxed focus:outline-none focus:ring-2 focus:ring-sky-500 bg-white ${
                  variant === "freeform" ? "flex-1 min-h-0" : "h-full"
                }`}
              />
            ) : isEditing ? (
              // 레거시 HTML — allow-same-origin so we can flip contentEditable +
              // read back the edited HTML; still no allow-scripts. Distinct key
              // forces a remount (so onLoad fires).
              <iframe
                key="body-editor"
                ref={editRef}
                sandbox="allow-same-origin"
                srcDoc={srcDocFor(html ?? "", theme)}
                onLoad={onEditLoad}
                title={t("detail.body.edit")}
                className={`w-full border-0 ${variant === "freeform" ? "flex-1 min-h-0 bg-transparent" : "h-full bg-white"}`}
              />
            ) : (
              // allow-same-origin (no allow-scripts) so the copy overlay can read
              // the rendered doc back; the LLM output still can't run scripts.
              <iframe
                key="body-viewer"
                ref={editRef}
                sandbox="allow-same-origin"
                srcDoc={srcDocFor(html ?? "", theme)}
                onLoad={variant === "freeform" ? undefined : adjustIframeHeight}
                scrolling={variant === "freeform" ? "yes" : "no"}
                title={t("detail.body.title")}
                className={`w-full border-0 ${variant === "freeform" ? "flex-1 min-h-0 bg-transparent" : "block bg-white"}`}
                style={variant === "freeform" ? undefined : { overflow: "hidden" }}
              />
            )}
          </div>
        ) : (
          <div className="flex flex-col items-center justify-center h-full">
            <FileText className="w-8 h-8 text-gray-300 mb-4" />
            <p className="text-sm text-gray-500">{t("detail.body.loadFail")}</p>
          </div>
        )}
      </div>
      {showVersions && (
        <VersionHistory
          noteId={noteId}
          theme={theme}
          onClose={() => setShowVersions(false)}
          onRestored={() => {
            setShowVersions(false);
            onRestored();
          }}
        />
      )}
      {showTranscript && transcriptId && (
        <TranscriptModal transcriptId={transcriptId} onClose={() => setShowTranscript(false)} />
      )}
    </section>
  );
}
