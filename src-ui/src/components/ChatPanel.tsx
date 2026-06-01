import { useState, useEffect, useRef, useCallback, useLayoutEffect } from "react";
import {
  ArrowRight,
  ArrowDown,
  ArrowLeft,
  FileText,
  MoreHorizontal,
  Trash2,
  AlertTriangle,
  RotateCcw,
  Mic,
  Square,
  X,
  Play,
  Pause,
  Paperclip,
  Archive,
} from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { toast } from "sonner";

import { chatApi, ChatMessage } from "@/api/chat";
import { recordingsApi, type Recording } from "@/api/recordings";
import { Note } from "@/api/notes";
import { processingApi, TimelineEvent } from "@/api/processing";
import AssistantMarkdown from "@/components/AssistantMarkdown";
import VersionHistory from "@/components/VersionHistory";
import NoteTags from "@/components/NoteTags";
import SourceSelector from "@/components/SourceSelector";
import WaveBars from "@/components/WaveBars";
import { getSavedSource } from "@/lib/audioDevice";
import { useLang } from "@/i18n/LangContext";
import type { DictKey } from "@/i18n/dict";

// 첨부 가능한 오디오/영상(오디오 추출) 확장자 — FileUploader와 동일.
const AUDIO_EXTS = ["mp3", "wav", "m4a", "webm", "ogg", "flac", "aac", "mp4", "mov", "mkv"];

const TOOL_LABEL: Record<string, DictKey> = {
  update_meeting_metadata: "chat.tool.updateMeta",
  refine_minutes: "chat.tool.refine",
  write_note: "chat.tool.writeNote",
  get_recording_download_url: "chat.tool.recordingUrl",
  read_transcript: "chat.tool.readTranscript",
  retry_transcribe: "chat.tool.retryTranscribe",
  retry_failed_task: "chat.tool.retryTask",
};

// transcribing 세부 step(finalize/transcribe/minutes) → 입력창 위 진행 카드의
// 짧은 라벨. step을 못 받으면 "전사 중"으로 fallback (a2d9a6f).
const TRANSCRIBING_STEP_LABEL: Record<string, DictKey> = {
  finalize: "chat.transcribing.finalize",
  transcribe: "chat.transcribing.transcribe",
  minutes: "chat.transcribing.minutes",
};

function formatElapsed(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const m = Math.floor(total / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(m)}:${pad(s)}`;
}


/**
 * Left-column panel — note header + chat agent (Phase 3). The top bar IS the
 * note header (back / title / meta / overflow menu), matching the old
 * MeetingChatPanel; the page has no separate header. A recording/transcribing
 * status bar sits just above the input. Chat is streaming: send runs the agent
 * to completion (`chat:status`/`chat:delta` events), then reloads persisted
 * messages.
 */
export default function ChatPanel({
  noteId,
  note,
  stage,
  elapsed = 0,
  progressPct = 0,
  transcribingStep,
  failureKind = null,
  onBack,
  onDelete,
  onRetry,
  onAfterTurn,
}: {
  noteId: string;
  note: Note;
  stage: string;
  elapsed?: number;
  progressPct?: number;
  transcribingStep?: string;
  failureKind?: "transcript" | "minutes" | null;
  onBack: () => void;
  onDelete: () => void;
  onRetry: () => void;
  onAfterTurn: () => void;
}) {
  const { t } = useLang();
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [pendingUser, setPendingUser] = useState<string | null>(null);
  // 전송 직후 낙관적으로 보여줄 첨부 칩(턴 끝나면 실제 메시지로 교체).
  const [pendingRecs, setPendingRecs] = useState<Recording[]>([]);
  const [showScrollBtn, setShowScrollBtn] = useState(false);
  const [streamText, setStreamText] = useState("");
  const [timeline, setTimeline] = useState<TimelineEvent[]>([]);
  const [showVersionId, setShowVersionId] = useState<string | null>(null);
  const [showMenu, setShowMenu] = useState(false);
  // freeform 녹음 첨부(Step 3, 멀티). recState=진행 중 녹음 1개의 단계,
  // attachedRecs=전송 대기 중인 첨부 녹음 목록(여러 개).
  const [recState, setRecState] = useState<"idle" | "recording" | "finalizing">("idle");
  const [attachedRecs, setAttachedRecs] = useState<Recording[]>([]);
  const [recPopover, setRecPopover] = useState(false);
  const [showLeaveWarn, setShowLeaveWarn] = useState(false);
  const [deleteRecId, setDeleteRecId] = useState<string | null>(null);
  // 현재 재생 중인 첨부 녹음 id(없으면 null) + 단일 Audio/blob URL 핸들.
  const [playingId, setPlayingId] = useState<string | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const audioUrlRef = useRef<string | null>(null);
  // 파일 업로드(📎/드래그) + 보관함(전송한 녹음 이력).
  const [importing, setImporting] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [archived, setArchived] = useState<Recording[]>([]);
  const [showArchive, setShowArchive] = useState(false);
  const importBusyRef = useRef(false);
  const [recSilent, setRecSilent] = useState(false);
  const lastSoundRef = useRef(0);
  const [recElapsed, setRecElapsed] = useState(0);
  const recIdRef = useRef<string | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLTextAreaElement | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);
  // Stick to bottom on new content unless the user scrolled up.
  const autoStickRef = useRef(true);
  // 전송 진행 중 여부의 최신값 미러(이벤트 리스너 클로저에서 읽기 위함).
  const sendingRef = useRef(false);

  // 녹음/업로드 진행 중이면 새 업로드(드래그 포함)를 막기 위한 최신값 미러.
  importBusyRef.current = recState !== "idle" || importing;
  sendingRef.current = sending;

  const reload = useCallback(() => {
    chatApi.list(noteId).then(setMessages).catch(() => {});
    processingApi.listTimeline(noteId).then(setTimeline).catch(() => {});
  }, [noteId]);

  // 타임라인만 갱신(전송 진행 중 메시지 reload로 인한 낙관적 버블 중복을 피함).
  const reloadTimeline = useCallback(() => {
    processingApi.listTimeline(noteId).then(setTimeline).catch(() => {});
  }, [noteId]);

  // 보관함(전송한 녹음) 로드 — 진입 시 + 전송 후 갱신.
  const loadArchived = useCallback(() => {
    if (stage !== "freeform") return;
    recordingsApi.listArchived(noteId).then(setArchived).catch(() => {});
  }, [noteId, stage]);

  useEffect(() => {
    reload();
  }, [reload]);

  // freeform: 진입 시 전송 대기(미소비) 녹음을 첨부 칩으로 복원한다. 앱을 끄거나
  // 노트를 벗어났다 돌아와도 보내지 않은 녹음이 유지된다(Step 3/4 recovery).
  useEffect(() => {
    if (stage !== "freeform") return;
    recordingsApi
      .listPending(noteId)
      .then(setAttachedRecs)
      .catch(() => {});
  }, [noteId, stage]);

  // 보관함 로드(진입 시).
  useEffect(() => {
    loadArchived();
  }, [loadArchived]);

  // 드래그앤드롭 업로드 — Tauri 네이티브 drag-drop은 실제 파일 경로를 준다(HTML5
  // drop은 못 줌). webview 전역 이벤트라 freeform일 때만 구독하고, 녹음/업로드
  // 중에는 무시한다. 최신 busy 상태는 ref로 읽어 재구독을 피한다.
  useEffect(() => {
    if (stage !== "freeform") return;
    let un: (() => void) | undefined;
    let alive = true;
    getCurrentWebview()
      .onDragDropEvent((e) => {
        if (importBusyRef.current) return;
        if (e.payload.type === "drop") {
          setIsDragging(false);
          const p = e.payload.paths?.[0];
          if (p) importAudioPath(p);
        } else if (e.payload.type === "leave") {
          setIsDragging(false);
        } else {
          setIsDragging(true); // enter / over
        }
      })
      .then((f) => {
        if (alive) un = f;
        else f();
      });
    return () => {
      alive = false;
      un?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stage, noteId]);

  // In-flight tool label + live streaming text deltas.
  useEffect(() => {
    const unStatus = listen<{
      note_id: string;
      state: string;
      tool?: string;
      current?: number;
      total?: number;
    }>("chat:status", (e) => {
      if (e.payload.note_id !== noteId) return;
      if (e.payload.state === "transcribing" || e.payload.state === "drafting") {
        const { current, total } = e.payload;
        const base =
          e.payload.state === "transcribing"
            ? t("chat.rec.transcribing")
            : t("chat.rec.drafting");
        setStatus(total && total > 1 ? `${base} (${current}/${total})` : base);
      } else if (e.payload.state === "merging") {
        setStatus(t("chat.rec.merging"));
      } else if (e.payload.state === "tool" && e.payload.tool) {
        setStatus(TOOL_LABEL[e.payload.tool] ? t(TOOL_LABEL[e.payload.tool]) : t("chat.processing"));
      } else {
        setStatus(t("chat.thinking"));
      }
    });
    const unDelta = listen<{ note_id: string; delta: string }>("chat:delta", (e) => {
      if (e.payload.note_id !== noteId) return;
      setStreamText((prev) => prev + e.payload.delta);
    });
    // Lifecycle(전사·회의록 생성)로 timeline이 늘면 백엔드가 note:updated를 쏜다.
    // 그걸 받아 reload해야 첫 채팅 입력 전에도 타임라인이 실시간으로 채워진다.
    // 단, 전송 진행 중엔 메시지 reload를 미뤄 낙관적 버블 중복을 막는다(전사 동안
    // note:updated가 여러 번 와도 timeline만 갱신; 메시지는 finally에서 한 번 reload).
    const unNote = listen("note:updated", () => {
      if (sendingRef.current) reloadTimeline();
      else reload();
    });
    return () => {
      unStatus.then((fn) => fn());
      unDelta.then((fn) => fn());
      unNote.then((fn) => fn());
    };
  }, [noteId, reload, reloadTimeline]);

  // Auto-grow textarea up to ~3 lines.
  useEffect(() => {
    const el = inputRef.current;
    if (el) {
      el.style.height = "auto";
      el.style.height = Math.min(el.scrollHeight, 96) + "px";
    }
  }, [input]);

  // 언마운트 시 재생 중인 오디오 정리(유령 재생·blob 누수 방지).
  useEffect(() => {
    return () => stopPlayback();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Focus the input on mount so the user can type right away on entering a note.
  // The textarea stays enabled while sending (send() guards against double-submit),
  // so focus is never dropped mid-turn and no re-focus dance is needed.
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Close the overflow menu on outside click.
  useEffect(() => {
    if (!showMenu) return;
    function onDocClick(e: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setShowMenu(false);
      }
    }
    document.addEventListener("mousedown", onDocClick);
    return () => document.removeEventListener("mousedown", onDocClick);
  }, [showMenu]);

  function handleScroll() {
    const el = scrollRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 16;
    autoStickRef.current = atBottom;
    setShowScrollBtn(!atBottom);
  }

  function scrollToBottom() {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
    autoStickRef.current = true;
    setShowScrollBtn(false);
  }

  // Auto-track the bottom — but only while the user hasn't scrolled away.
  useLayoutEffect(() => {
    if (autoStickRef.current && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages.length, timeline.length, pendingUser, status, streamText]);

  const send = async () => {
    const text = input.trim();
    const recIds = attachedRecs.map((r) => r.id);
    // 텍스트나 첨부 녹음(1개 이상) 중 하나는 있어야 하고, 녹음/정리 중엔 전송 불가.
    if ((!text && recIds.length === 0) || sending || recState === "recording" || recState === "finalizing") return;
    stopPlayback(); // 첨부를 보내면 재생 중이던 녹음을 멈춘다.
    setInput("");
    setSending(true);
    setPendingUser(text || null);
    setPendingRecs(attachedRecs); // 낙관적 칩(턴 끝나면 실제 메시지 칩으로 교체)
    setStatus(t("chat.thinking"));
    setStreamText("");
    // 첨부는 전송 즉시 비운다(전사·종합은 백엔드가 처리 — Step 4).
    setAttachedRecs([]);
    try {
      await chatApi.send(noteId, text, recIds.length ? { stage, recordingIds: recIds } : { stage });
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSending(false);
      setStatus(null);
      setStreamText("");
      // 실제 메시지를 불러온 뒤에 낙관적 버블(텍스트·칩)을 치워 깜빡임을 막는다.
      chatApi
        .list(noteId)
        .then(setMessages)
        .catch(() => {})
        .finally(() => {
          setPendingUser(null);
          setPendingRecs([]);
        });
      processingApi.listTimeline(noteId).then(setTimeline).catch(() => {});
      // 성공 시 백엔드가 첨부를 consumed 처리 → 빈 목록. 실패 시 미소비 녹음이
      // 그대로 남아 칩으로 즉시 복원된다(보낸 녹음 유실 방지). 보관함도 갱신.
      if (stage === "freeform") {
        recordingsApi.listPending(noteId).then(setAttachedRecs).catch(() => {});
        loadArchived();
      }
      onAfterTurn();
    }
  };

  // ── freeform 녹음 첨부 (Step 3) ──────────────────────────────────────
  const startRec = async () => {
    const src = getSavedSource();
    if (!src) {
      toast.error(t("chat.rec.noSource"));
      return;
    }
    try {
      const rec = await recordingsApi.start(noteId, src.name, src.source);
      recIdRef.current = rec.id;
      setRecElapsed(0);
      setRecState("recording");
      setRecPopover(false);
    } catch (e) {
      toast.error(String(e));
    }
  };
  const stopRec = async () => {
    const id = recIdRef.current;
    if (!id) return;
    setRecState("finalizing");
    try {
      await recordingsApi.stop(id);
      // freeform은 자동 전사를 안 타고 finalize(webm)까지만 — 폴링으로 완료 확인.
      for (let i = 0; i < 90; i++) {
        await new Promise((r) => setTimeout(r, 1000));
        const rec = await recordingsApi.get(id);
        if (rec.finalized_at || rec.format === "webm") {
          setAttachedRecs((prev) => [...prev, rec]);
          setRecState("idle");
          recIdRef.current = null;
          return;
        }
      }
      toast.error(t("chat.rec.finalizeTimeout"));
      setRecState("idle");
    } catch (e) {
      toast.error(String(e));
      setRecState("idle");
    }
  };
  // 진행 중인 녹음을 취소(뒤로가기 경고에서 사용).
  const cancelRec = () => {
    const id = recIdRef.current;
    recIdRef.current = null;
    setRecElapsed(0);
    setRecState("idle");
    if (id) recordingsApi.delete(id).catch(() => {});
  };
  // 첨부 칩의 X — 칩은 저장·복원되는 실제 녹음이므로 확인 모달을 먼저 띄운다.
  const requestRemoveAttached = (id: string) => setDeleteRecId(id);
  // 확인 시 목록에서 빼고 DB 행+파일까지 완전 삭제. 입력 포커스 유지.
  const confirmRemoveAttached = () => {
    const id = deleteRecId;
    setDeleteRecId(null);
    if (!id) return;
    if (playingId === id) stopPlayback();
    // 첨부 칩이든 보관함 항목이든 같은 모달을 쓰므로 양쪽 목록에서 모두 제거.
    setAttachedRecs((prev) => prev.filter((r) => r.id !== id));
    setArchived((prev) => prev.filter((r) => r.id !== id));
    recordingsApi.delete(id).catch(() => {});
    inputRef.current?.focus();
  };
  // 진행 중인 재생을 멈추고 핸들/blob URL을 정리한다.
  const stopPlayback = () => {
    if (audioRef.current) {
      audioRef.current.pause();
      audioRef.current.src = "";
      audioRef.current = null;
    }
    if (audioUrlRef.current) {
      URL.revokeObjectURL(audioUrlRef.current);
      audioUrlRef.current = null;
    }
    setPlayingId(null);
  };
  // 첨부 녹음 재생 — 바이트를 읽어 Blob URL로 재생(asset 프로토콜 불필요).
  // 같은 칩을 다시 누르면 정지(토글), 다른 칩을 누르면 기존 재생을 멈추고 전환.
  const playRec = async (id: string) => {
    if (playingId === id) {
      stopPlayback();
      return;
    }
    stopPlayback();
    try {
      const buf = await recordingsApi.readAudio(id);
      const url = URL.createObjectURL(new Blob([buf], { type: "audio/webm" }));
      const audio = new Audio(url);
      audioRef.current = audio;
      audioUrlRef.current = url;
      setPlayingId(id);
      audio.onended = stopPlayback;
      audio.onerror = stopPlayback;
      await audio.play();
    } catch (e) {
      stopPlayback();
      toast.error(String(e));
    }
  };
  // 파일 경로를 webm으로 가져와 첨부 칩으로 추가(freeform은 전사 skip — 전송 때 처리).
  const importAudioPath = async (path: string) => {
    if (importBusyRef.current && !importing) return;
    setImporting(true);
    try {
      const rec = await recordingsApi.importAudioFile(noteId, path);
      setAttachedRecs((prev) => [...prev, rec]);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setImporting(false);
    }
  };
  // 📎 버튼 — 파일 선택 다이얼로그.
  const pickAudioFile = async () => {
    if (importBusyRef.current) return;
    try {
      const sel = await open({
        multiple: false,
        filters: [{ name: t("uploader.audio"), extensions: AUDIO_EXTS }],
      });
      if (typeof sel === "string") await importAudioPath(sel);
    } catch (e) {
      toast.error(String(e));
    }
  };

  // 녹음 중 뒤로가기 → 경고. 나가면 진행 중 녹음을 취소한다.
  const handleBack = () => {
    if (recState === "recording") {
      setShowLeaveWarn(true);
      return;
    }
    onBack();
  };
  const confirmLeave = () => {
    setShowLeaveWarn(false);
    cancelRec();
    onBack();
  };
  // 녹음 중 경과 타이머.
  useEffect(() => {
    if (recState !== "recording") {
      setRecSilent(false);
      return;
    }
    lastSoundRef.current = Date.now();
    setRecSilent(false);
    const timer = setInterval(() => {
      setRecElapsed((e) => e + 1);
      // 3초 넘게 소리가 안 잡히면 "감지 안 됨" 경고.
      setRecSilent(Date.now() - lastSoundRef.current > 3000);
    }, 1000);
    return () => clearInterval(timer);
  }, [recState]);

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      send();
    }
  };

  // Interleave chat messages + lifecycle timeline events into one chronological
  // feed (matches the old MeetingChatPanel).
  const feed = [
    ...messages
      // 내용 있는 메시지 + 첨부 녹음만 있는(내용 빈) 유저 메시지를 남긴다.
      .filter((m) => m.content.trim() || (m.role === "user" && (m.recordings?.length ?? 0) > 0))
      .map((m) => ({ k: "msg" as const, at: m.created_at, m })),
    // "녹음이 정리되었습니다"(recording_stopped)는 항상 숨기고, freeform 첨부 전송의
    // 전사 시작/완료 pill도 숨긴다(자체 status·결과 메시지로 대체).
    ...timeline
      .filter((ev) => {
        if (ev.kind === "recording_stopped") return false;
        if (
          stage === "freeform" &&
          (ev.kind === "transcribe_started" || ev.kind === "transcribe_completed")
        )
          return false;
        return true;
      })
      .map((ev) => ({ k: "event" as const, at: ev.created_at, ev })),
  ].sort((a, b) => (a.at < b.at ? -1 : 1));

  // 유저 말풍선/낙관적 버블에서 쓰는 재생 전용 첨부 칩(🎤+분초+재생 토글).
  const renderRecChip = (rec: Recording) => (
    <div
      key={rec.id}
      className="flex items-center gap-1.5 bg-gray-100 rounded-lg pl-2 pr-1 py-1"
    >
      <Mic size={14} className="text-sky-600 shrink-0" />
      <span className="text-xs font-medium text-gray-700 tabular-nums shrink-0">
        {formatElapsed(Math.round(rec.duration ?? 0))}
      </span>
      <button
        onClick={() => playRec(rec.id)}
        title={playingId === rec.id ? t("chat.rec.pause") : t("chat.rec.play")}
        className={`p-0.5 hover:bg-gray-200 rounded cursor-pointer shrink-0 ${
          playingId === rec.id ? "text-sky-600" : "text-gray-400 hover:text-sky-600"
        }`}
      >
        {playingId === rec.id ? (
          <Pause size={12} fill="currentColor" />
        ) : (
          <Play size={12} fill="currentColor" />
        )}
      </button>
    </div>
  );

  return (
    <aside className="border-r border-gray-200 bg-white flex flex-col h-full min-h-0 relative">
      {/* 드래그 업로드 오버레이 — 파일을 패널 위로 끌어오면 표시. */}
      {stage === "freeform" && isDragging && (
        <div className="absolute inset-0 z-40 m-2 bg-sky-50/90 border-2 border-dashed border-sky-400 rounded-xl flex flex-col items-center justify-center pointer-events-none">
          <Paperclip className="w-8 h-8 text-sky-500 mb-2" strokeWidth={1.5} />
          <p className="text-sm font-medium text-sky-700">{t("chat.rec.dropHint")}</p>
        </div>
      )}
      {/* Note header — back / title / meta / overflow menu (was the page header). */}
      <div className="px-5 pt-4 pb-3 border-b border-gray-100 shrink-0 bg-gray-50 flex items-center gap-3">
        <button
          onClick={handleBack}
          className="p-1.5 -ml-1.5 text-gray-500 hover:text-gray-900 hover:bg-gray-200 rounded-md transition-colors bg-transparent border-0 cursor-pointer shrink-0"
          title={t("chat.back")}
          aria-label={t("chat.back")}
        >
          <ArrowLeft size={18} />
        </button>
        <div className="flex-1 min-w-0">
          <div className="text-base font-medium text-gray-900 leading-tight mb-1 truncate">{note.title}</div>
          <NoteTags noteId={note.id} />
        </div>
        {stage === "freeform" && archived.length > 0 && (
          <div className="relative shrink-0">
            <button
              onClick={() => setShowArchive((v) => !v)}
              className="flex items-center gap-1 px-2 py-1 text-gray-500 hover:text-gray-900 hover:bg-gray-200 rounded-md transition-colors bg-transparent border-0 cursor-pointer"
              title={t("chat.rec.archive")}
            >
              <Archive size={16} />
              <span className="text-xs font-medium tabular-nums">{archived.length}</span>
            </button>
            {showArchive && (
              <>
                <div className="fixed inset-0 z-20" onClick={() => setShowArchive(false)} />
                <div className="absolute right-0 top-full mt-1 z-30 w-64 bg-white border border-gray-200 rounded-xl shadow-lg p-2">
                  <div className="px-2 py-1.5 text-xs font-medium text-gray-500">
                    {t("chat.rec.archiveTitle")}
                  </div>
                  <div className="max-h-64 overflow-y-auto space-y-1">
                    {archived.map((rec) => (
                      <div
                        key={rec.id}
                        className="flex items-center gap-1.5 px-2 py-1.5 rounded-lg hover:bg-gray-50"
                      >
                        <Mic size={14} className="text-sky-600 shrink-0" />
                        <span className="text-xs font-medium text-gray-700 tabular-nums flex-1">
                          {formatElapsed(Math.round(rec.duration ?? 0))}
                        </span>
                        <button
                          onClick={() => playRec(rec.id)}
                          title={playingId === rec.id ? t("chat.rec.pause") : t("chat.rec.play")}
                          className={`p-0.5 hover:bg-gray-200 rounded cursor-pointer shrink-0 ${
                            playingId === rec.id ? "text-sky-600" : "text-gray-400 hover:text-sky-600"
                          }`}
                        >
                          {playingId === rec.id ? (
                            <Pause size={12} fill="currentColor" />
                          ) : (
                            <Play size={12} fill="currentColor" />
                          )}
                        </button>
                        <button
                          onClick={() => requestRemoveAttached(rec.id)}
                          title={t("chat.rec.cancel")}
                          className="p-0.5 text-gray-400 hover:text-red-600 hover:bg-gray-200 rounded cursor-pointer shrink-0"
                        >
                          <Trash2 size={12} />
                        </button>
                      </div>
                    ))}
                  </div>
                </div>
              </>
            )}
          </div>
        )}
        <div ref={menuRef} className="relative shrink-0">
          <button
            onClick={() => setShowMenu((v) => !v)}
            className="p-1.5 text-gray-500 hover:text-gray-900 hover:bg-gray-200 rounded-md transition-colors bg-transparent border-0 cursor-pointer"
            title={t("chat.more")}
            aria-label={t("chat.more")}
          >
            <MoreHorizontal size={18} />
          </button>
          {showMenu && (
            <div className="absolute right-0 top-full mt-1 z-20 bg-white border border-gray-200 rounded-lg shadow-lg py-1 w-44">
              <button
                onClick={() => {
                  setShowMenu(false);
                  onDelete();
                }}
                className="flex items-center gap-2 w-full text-left px-3 py-2 text-sm text-red-600 hover:bg-red-50 bg-transparent border-0 cursor-pointer"
              >
                <Trash2 size={14} />
                <span>{t("chat.delete")}</span>
              </button>
            </div>
          )}
        </div>
      </div>

      <div ref={scrollRef} onScroll={handleScroll} className="flex-1 overflow-y-auto py-5 px-4 space-y-5 min-h-0">
        {feed.map((it) => {
          if (it.k === "event") {
            return (
              <div key={`e-${it.ev.id}`} className="flex justify-center">
                <span className="inline-flex items-center px-3 py-1 rounded-full text-[11px] text-gray-500 bg-gray-50 border border-gray-200">
                  {it.ev.content}
                </span>
              </div>
            );
          }
          const m = it.m;
          if (m.role === "user") {
            const recs = m.recordings ?? [];
            return (
              <div key={m.id} className="flex justify-end">
                <div className="max-w-[85%] flex flex-col items-end gap-1.5">
                  {m.content.trim() && (
                    <div className="bg-gray-100 text-gray-800 text-sm px-4 py-2.5 rounded-2xl whitespace-pre-wrap leading-relaxed">
                      {m.content}
                    </div>
                  )}
                  {recs.length > 0 && (
                    <div className="flex flex-wrap gap-1.5 justify-end">
                      {recs.map(renderRecChip)}
                    </div>
                  )}
                </div>
              </div>
            );
          }
          return (
            <div key={m.id} className="flex justify-start" data-role="assistant">
              <div className="max-w-[90%] space-y-2">
                <AssistantMarkdown text={m.content} />
                {m.note_body_version_id && (
                  <button
                    onClick={() => setShowVersionId(m.note_body_version_id)}
                    className="inline-flex items-center gap-1.5 px-2.5 py-1 text-[11px] text-sky-700 bg-sky-50 hover:bg-sky-100 rounded-lg border border-sky-200 cursor-pointer transition-colors"
                    title={t("chat.viewVersion.title")}
                  >
                    <FileText size={11} />
                    <span>{t("chat.viewVersion")}</span>
                  </button>
                )}
              </div>
            </div>
          );
        })}

        {(pendingUser || pendingRecs.length > 0) && (
          <div className="flex justify-end">
            <div className="max-w-[85%] flex flex-col items-end gap-1.5">
              {pendingUser && (
                <div className="bg-gray-100 text-gray-800 text-sm px-4 py-2.5 rounded-2xl whitespace-pre-wrap leading-relaxed">
                  {pendingUser}
                </div>
              )}
              {pendingRecs.length > 0 && (
                <div className="flex flex-wrap gap-1.5 justify-end">
                  {pendingRecs.map(renderRecChip)}
                </div>
              )}
            </div>
          </div>
        )}

        {sending && (
          <div className="flex justify-start">
            <div className="max-w-[90%] space-y-2">
              {streamText ? (
                <AssistantMarkdown text={streamText} />
              ) : (
                <div className="flex items-center gap-1 py-1.5">
                  <span className="w-2 h-2 bg-gray-400 rounded-full animate-pulse" style={{ animationDelay: "0ms" }} />
                  <span className="w-2 h-2 bg-gray-400 rounded-full animate-pulse" style={{ animationDelay: "200ms" }} />
                  <span className="w-2 h-2 bg-gray-400 rounded-full animate-pulse" style={{ animationDelay: "400ms" }} />
                </div>
              )}
              {status && <div className="text-[11px] text-gray-400 italic">{status}</div>}
            </div>
          </div>
        )}
      </div>

      {showScrollBtn && (
        <button
          onClick={scrollToBottom}
          className="absolute left-1/2 -translate-x-1/2 bottom-24 z-10 w-8 h-8 rounded-full bg-white border border-gray-300 shadow-md text-gray-600 hover:bg-gray-50 hover:text-gray-900 transition-colors flex items-center justify-center cursor-pointer"
          title={t("chat.scrollLatest")}
          aria-label={t("chat.scrollLatest")}
        >
          <ArrowDown size={16} />
        </button>
      )}

      <div className="shrink-0 px-4 pb-4 pt-2 space-y-2">
        {/* Recording / transcribing status bar — above the input (matches old). */}
        {stage === "recording" && (
          <div className="flex items-center gap-2 px-3 py-2 bg-red-50 border border-red-100 rounded-lg">
            <span className="w-2 h-2 bg-red-500 rounded-full animate-pulse shrink-0" />
            <span className="text-xs font-medium text-red-700">{t("chat.recording")}</span>
            <span className="text-xs font-mono text-red-700 tabular-nums">{formatElapsed(elapsed)}</span>
          </div>
        )}
        {stage === "transcribing" && failureKind && (
          <div className="flex items-center gap-2 px-3 py-2 bg-amber-50 border border-amber-200 rounded-lg">
            <AlertTriangle size={14} className="text-amber-500 shrink-0" />
            <span className="text-xs font-medium text-amber-700 flex-1 truncate">
              {failureKind === "minutes" ? t("chat.fail.minutes") : t("chat.fail.transcribe")}
            </span>
            <button
              onClick={onRetry}
              className="inline-flex items-center gap-1 px-2 py-0.5 text-[11px] font-medium text-amber-800 bg-white border border-amber-300 rounded hover:bg-amber-100 cursor-pointer shrink-0"
            >
              <RotateCcw size={11} />
              {t("chat.retry")}
            </button>
          </div>
        )}
        {stage === "transcribing" && !failureKind && (
          <div className="flex items-center gap-2 px-3 py-2 bg-sky-50 border border-sky-100 rounded-lg">
            <div className="w-3 h-3 border-2 border-sky-300 border-t-sky-600 rounded-full animate-spin shrink-0" />
            <span className="text-xs font-medium text-sky-700">
              {t(TRANSCRIBING_STEP_LABEL[transcribingStep ?? "transcribe"] ?? "chat.transcribing.transcribe")}
            </span>
            <span className="text-xs text-sky-700 tabular-nums">{progressPct}%</span>
            <div className="flex-1 h-1 bg-sky-100 rounded-full overflow-hidden ml-2">
              <div className="h-full bg-sky-600 transition-all duration-500" style={{ width: `${progressPct}%` }} />
            </div>
          </div>
        )}
        {/* freeform 녹음 첨부 status (Step 3) — 녹음 중 / 정리 중 / 첨부됨. */}
        {stage === "freeform" && recState === "recording" && (
          <div className="px-3 py-2 bg-red-50 border border-red-100 rounded-lg space-y-1.5">
            <div className="flex items-center gap-2">
              <span className="w-2 h-2 bg-red-500 rounded-full animate-pulse shrink-0" />
              <span className="text-xs font-medium text-red-700">{t("chat.recording")}</span>
              <span className="text-xs font-mono text-red-700 tabular-nums">{formatElapsed(recElapsed)}</span>
              <button
                onClick={stopRec}
                className="ml-auto inline-flex items-center gap-1 px-2.5 py-1 text-[11px] font-medium text-white bg-red-500 hover:bg-red-600 rounded-md border-0 cursor-pointer shrink-0"
              >
                <Square size={10} fill="currentColor" />
                {t("chat.rec.stop")}
              </button>
            </div>
            <div className="h-6">
              <WaveBars
                targetId={recIdRef.current ?? ""}
                fill
                maxHeight={24}
                minHeight={2}
                gap={2}
                color="bg-red-400"
                onLevel={(p) => {
                  if (p > 0.06) lastSoundRef.current = Date.now();
                }}
              />
            </div>
            {recSilent && (
              <p className="text-[11px] text-amber-600 text-center">{t("chat.rec.noSound")}</p>
            )}
          </div>
        )}
        <div className="flex flex-col gap-1.5 bg-white border border-[#dbdee3] rounded-2xl px-4 py-2 shadow-md focus-within:border-sky-300 transition-colors">
          {/* 멀티미디어 첨부 칩(여러 개) + 정리 중(원형 ring). 입력창 안에 붙는다. */}
          {stage === "freeform" && (attachedRecs.length > 0 || recState === "finalizing") && (
            <div className="flex flex-wrap gap-1.5">
              {attachedRecs.map((rec) => (
                <div
                  key={rec.id}
                  className="flex items-center gap-1.5 bg-gray-100 rounded-lg pl-2 pr-1 py-1 max-w-full"
                >
                  <Mic size={14} className="text-sky-600 shrink-0" />
                  <span className="text-xs font-medium text-gray-700 tabular-nums shrink-0">
                    {formatElapsed(Math.round(rec.duration ?? 0))}
                  </span>
                  <button
                    onClick={() => playRec(rec.id)}
                    className={`p-0.5 hover:bg-gray-200 rounded cursor-pointer shrink-0 ${
                      playingId === rec.id ? "text-sky-600" : "text-gray-400 hover:text-sky-600"
                    }`}
                    title={playingId === rec.id ? t("chat.rec.pause") : t("chat.rec.play")}
                  >
                    {playingId === rec.id ? (
                      <Pause size={12} fill="currentColor" />
                    ) : (
                      <Play size={12} fill="currentColor" />
                    )}
                  </button>
                  <button
                    onClick={() => requestRemoveAttached(rec.id)}
                    className="p-0.5 text-gray-400 hover:text-gray-700 hover:bg-gray-200 rounded cursor-pointer shrink-0"
                    title={t("chat.rec.cancel")}
                  >
                    <X size={13} />
                  </button>
                </div>
              ))}
              {recState === "finalizing" && (
                <div className="flex items-center gap-1.5 bg-gray-100 rounded-lg pl-2 pr-2.5 py-1">
                  <span className="w-4 h-4 rounded-full border-2 border-gray-300 border-t-sky-500 animate-spin shrink-0" />
                  <span className="text-xs font-medium text-gray-700">{t("chat.rec.finalizing")}</span>
                </div>
              )}
            </div>
          )}
          <div className="flex items-center gap-2 min-h-[30px]">
          <textarea
            ref={inputRef}
            rows={1}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder={t("chat.input.placeholder")}
            className="flex-1 text-sm text-gray-700 bg-transparent border-0 outline-none resize-none placeholder:text-gray-400 max-h-24 overflow-y-auto leading-6"
          />
          {stage === "freeform" && recState === "idle" && (
            <button
              onClick={pickAudioFile}
              disabled={importing}
              className="p-1.5 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded-lg border-0 bg-transparent cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed shrink-0"
              title={t("chat.rec.upload")}
            >
              {importing ? (
                <span className="block w-[18px] h-[18px] rounded-full border-2 border-gray-300 border-t-sky-500 animate-spin" />
              ) : (
                <Paperclip size={18} />
              )}
            </button>
          )}
          {stage === "freeform" && recState === "idle" && (
            <div className="relative shrink-0">
              <button
                onClick={() => setRecPopover((v) => !v)}
                className="p-1.5 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded-lg border-0 bg-transparent cursor-pointer"
                title={t("chat.rec.record")}
              >
                <Mic size={18} />
              </button>
              {recPopover && (
                <>
                  <div className="fixed inset-0 z-20" onClick={() => setRecPopover(false)} />
                  <div className="absolute bottom-full right-0 mb-2 z-30 w-72 bg-white border border-gray-200 rounded-xl shadow-lg p-3">
                    <SourceSelector testMode="modal" />
                    <button
                      onClick={startRec}
                      className="mt-3 w-full flex items-center justify-center gap-2 px-4 py-2 bg-red-500 hover:bg-red-600 text-white rounded-lg text-sm font-medium border-0 cursor-pointer"
                    >
                      <span className="w-2 h-2 bg-white rounded-full" />
                      {t("chat.rec.start")}
                    </button>
                  </div>
                </>
              )}
            </div>
          )}
          {(() => {
            const canSend =
              (!!input.trim() || attachedRecs.length > 0) &&
              !sending &&
              recState !== "recording" &&
              recState !== "finalizing";
            return (
              <button
                onClick={send}
                disabled={!canSend}
                className={`p-1.5 rounded-lg border-0 shrink-0 ${
                  canSend
                    ? "bg-sky-500 text-white hover:bg-sky-600 cursor-pointer"
                    : "bg-sky-300 text-white cursor-not-allowed"
                }`}
                title={t("chat.send")}
              >
                <ArrowRight size={16} />
              </button>
            );
          })()}
          </div>
        </div>
      </div>

      {showVersionId && (
        <VersionHistory
          noteId={noteId}
          initialVersionId={showVersionId}
          onClose={() => setShowVersionId(null)}
          onRestored={() => {
            setShowVersionId(null);
            onAfterTurn();
          }}
        />
      )}

      {/* 녹음 중 뒤로가기 경고 (Step 3). */}
      {showLeaveWarn && (
        <div
          className="fixed inset-0 bg-black/40 z-50 flex items-center justify-center p-6 backdrop-blur-[2px]"
          onClick={() => setShowLeaveWarn(false)}
        >
          <div
            className="bg-white rounded-xl shadow-xl p-5 max-w-xs w-full"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="text-base font-medium text-gray-900 mb-1.5">{t("chat.rec.leaveTitle")}</h3>
            <p className="text-sm text-gray-500 leading-relaxed mb-4">{t("chat.rec.leaveDesc")}</p>
            <div className="flex gap-2 justify-end">
              <button
                onClick={() => setShowLeaveWarn(false)}
                className="px-3 py-1.5 text-sm font-medium text-gray-600 bg-gray-100 hover:bg-gray-200 rounded-lg border-0 cursor-pointer"
              >
                {t("chat.rec.leaveStay")}
              </button>
              <button
                onClick={confirmLeave}
                className="px-3 py-1.5 text-sm font-medium text-white bg-red-500 hover:bg-red-600 rounded-lg border-0 cursor-pointer"
              >
                {t("chat.rec.leaveLeave")}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 첨부 녹음 삭제 확인 (Step 3/4) — 복원되는 실제 녹음이라 완전 삭제 전 확인. */}
      {deleteRecId && (
        <div
          className="fixed inset-0 bg-black/40 z-50 flex items-center justify-center p-6 backdrop-blur-[2px]"
          onClick={() => setDeleteRecId(null)}
        >
          <div
            className="bg-white rounded-xl shadow-xl p-5 max-w-xs w-full"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="text-base font-medium text-gray-900 mb-1.5">{t("chat.rec.delTitle")}</h3>
            <p className="text-sm text-gray-500 leading-relaxed mb-4">{t("chat.rec.delDesc")}</p>
            <div className="flex gap-2 justify-end">
              <button
                onClick={() => setDeleteRecId(null)}
                className="px-3 py-1.5 text-sm font-medium text-gray-600 bg-gray-100 hover:bg-gray-200 rounded-lg border-0 cursor-pointer"
              >
                {t("chat.rec.delCancel")}
              </button>
              <button
                onClick={confirmRemoveAttached}
                className="px-3 py-1.5 text-sm font-medium text-white bg-red-500 hover:bg-red-600 rounded-lg border-0 cursor-pointer"
              >
                {t("chat.rec.delConfirm")}
              </button>
            </div>
          </div>
        </div>
      )}
    </aside>
  );
}
