import { useState, useEffect, useCallback, useRef } from "react";
import {
  RotateCw,
  Settings,
  X,
  Play,
  Square,
  Loader2,
  ChevronDown,
  Check,
  Volume2,
} from "lucide-react";
import { toast } from "sonner";

import { audioApi, AudioDeviceInfo } from "@/api/audio";
import { getSavedSource, saveSource, SelectedSource } from "@/lib/audioDevice";
import WaveBars from "@/components/WaveBars";
import { useT } from "@/i18n/LangContext";
import type { DictKey } from "@/i18n/dict";

// Design tokens lifted from the existing frontend's *user-facing* surfaces
// (MeetingsPage search/filter pills + MinutesVersionHistory list/popover),
// NOT the admin native <select>. The user sees these screens, so they match.
const ICON_BTN_CLS =
  "shrink-0 p-2 text-gray-500 hover:text-gray-900 hover:bg-gray-100 rounded-md transition-colors bg-transparent border-0 cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed";
// Section headers (입력 소스 선택 / 입력 소스 테스트 / 녹음 재생) — larger &
// darker than the sub-labels below them so each is read as its own section.
const CAPTION_CLS = "text-sm font-medium text-gray-900 mb-4";
// Sub-labels under a section (입력 파형 / 입력 레벨) — quiet, matching tone.
const SUBLABEL_CLS = "text-xs text-gray-500";

/**
 * Unified capture-source picker + settings (D-023).
 *
 * One "입력 소스 설정" panel — source dropdown, OS input volume, and the input
 * test — shared by two surfaces:
 * - testMode="modal" (note recording screen, default): a compact dropdown + ⚙
 *   row; the ⚙ opens the full panel in a modal so the busy screen stays tidy.
 * - testMode="inline" (Settings → 오디오): the same panel rendered expanded.
 *
 * Source selection persists to localStorage (lib/audioDevice); volume is the
 * real Windows endpoint volume (audio_volume.rs), so both surfaces and the OS
 * stay in sync automatically.
 */
export default function SourceSelector({
  testMode = "modal",
}: {
  testMode?: "modal" | "inline";
}) {
  const t = useT();
  const [devices, setDevices] = useState<AudioDeviceInfo[]>([]);
  const [selected, setSelected] = useState<SelectedSource | null>(getSavedSource());
  const [loading, setLoading] = useState(true);
  const [err, setErr] = useState<string | null>(null);
  const [open, setOpen] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setErr(null);
    try {
      const list = (await audioApi.listDevices()).filter((d) => d.capturable);
      setDevices(list);
      const saved = getSavedSource();
      const ok = saved && list.some((d) => d.name === saved.name && d.source === saved.source);
      if (!ok) {
        const def = list.find((d) => d.is_default && d.source === "mic") ?? list[0] ?? null;
        if (def) {
          const next = { name: def.name, source: def.source };
          setSelected(next);
          saveSource(next);
        } else {
          setSelected(null);
        }
      } else {
        setSelected(saved);
      }
    } catch (e) {
      setErr(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const onPick = (d: AudioDeviceInfo) => {
    const next: SelectedSource = { name: d.name, source: d.source };
    setSelected(next);
    saveSource(next);
  };

  if (err) return <p className="text-sm text-red-600">{err}</p>;

  if (devices.length === 0 && !loading) {
    return (
      <div className="flex items-center gap-2">
        <p className="flex-1 text-sm text-amber-700 bg-amber-50 border border-amber-200 rounded-md px-3 py-2">
          {t("source.empty")}
        </p>
        <button onClick={refresh} title={t("source.refresh")} disabled={loading} className={ICON_BTN_CLS}>
          <RotateCw className={`w-4 h-4 ${loading ? "animate-spin" : ""}`} />
        </button>
      </div>
    );
  }

  const panel = (
    <InputSourcePanel
      devices={devices}
      selected={selected}
      loading={loading}
      onPick={onPick}
      refresh={refresh}
    />
  );

  // Settings → 오디오: the full panel, expanded.
  if (testMode === "inline") return panel;

  // Note screen: quick dropdown + ⚙ that opens the same panel in a modal.
  return (
    <>
      <div className="flex items-center gap-2">
        <div className="flex-1 min-w-0">
          <SourceDropdown
            devices={devices}
            selected={selected}
            loading={loading}
            onPick={onPick}
            refresh={refresh}
          />
        </div>
        <button
          onClick={() => setOpen(true)}
          title={t("source.settings")}
          disabled={!selected}
          className={ICON_BTN_CLS}
        >
          <Settings className="w-4 h-4" />
        </button>
      </div>

      {open && (
        <div
          className="fixed inset-0 bg-black/40 z-50 flex items-center justify-center p-6 backdrop-blur-[2px]"
          onClick={() => setOpen(false)}
        >
          <div
            className="relative bg-white rounded-lg shadow-xl p-6 max-w-sm w-full mx-4 max-h-[90vh] overflow-y-auto"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between mb-5">
              <h3 className="text-base font-medium text-gray-900">{t("source.settings")}</h3>
              <button
                onClick={() => setOpen(false)}
                className="p-1.5 text-gray-400 hover:text-gray-900 hover:bg-gray-100 rounded-md transition-colors bg-transparent border-0 cursor-pointer"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
            {panel}
          </div>
        </div>
      )}
    </>
  );
}

// ============================================================================
// Panel — source select + OS volume + input test. Shared by modal & inline.
// ============================================================================

function InputSourcePanel({
  devices,
  selected,
  loading,
  onPick,
  refresh,
}: {
  devices: AudioDeviceInfo[];
  selected: SelectedSource | null;
  loading: boolean;
  onPick: (d: AudioDeviceInfo) => void;
  refresh: () => void;
}) {
  const t = useT();
  return (
    <div>
      <p className={CAPTION_CLS}>{t("source.select")}</p>
      <SourceDropdown
        devices={devices}
        selected={selected}
        loading={loading}
        onPick={onPick}
        refresh={refresh}
      />
      {selected && <VolumeSlider source={selected} />}

      <hr className="my-5 border-gray-100" />

      <p className={CAPTION_CLS}>{t("source.test")}</p>
      {selected ? (
        <InputTestBody source={selected} />
      ) : (
        <p className="text-sm text-gray-400">{t("source.selectFirst")}</p>
      )}
    </div>
  );
}

// ============================================================================
// Source dropdown — pill trigger + popover (self-contained open state, so the
// note row and the modal panel can each own one without colliding).
// ============================================================================

function SourceDropdown({
  devices,
  selected,
  loading,
  onPick,
  refresh,
}: {
  devices: AudioDeviceInfo[];
  selected: SelectedSource | null;
  loading: boolean;
  onPick: (d: AudioDeviceInfo) => void;
  refresh: () => void;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const selectedDevice = selected
    ? devices.find((d) => d.name === selected.name && d.source === selected.source)
    : undefined;

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((v) => !v)}
        disabled={loading}
        className="w-full flex items-center justify-between gap-2 bg-white border border-gray-200 rounded-full pl-4 pr-3 py-2 text-sm text-gray-800 hover:border-gray-300 focus:border-sky-300 outline-none transition-colors cursor-pointer disabled:opacity-60"
      >
        <span className="truncate">
          {selectedDevice ? (
            <>
              {selectedDevice.source === "mic" ? "🎤 " : "🔊 "}
              {selectedDevice.name}
            </>
          ) : (
            t("source.select")
          )}
        </span>
        <ChevronDown
          className={`w-4 h-4 text-gray-400 shrink-0 transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>

      {open && (
        <div className="absolute left-0 right-0 top-full mt-1.5 z-50 bg-white border border-gray-100 rounded-xl shadow-lg p-1.5 max-h-72 overflow-y-auto">
          {devices.map((d, i) => {
            const isSel = selected && d.name === selected.name && d.source === selected.source;
            return (
              <button
                key={i}
                onClick={() => {
                  onPick(d);
                  setOpen(false);
                }}
                className={`w-full text-left rounded-md px-3 py-2 text-sm cursor-pointer transition-colors flex items-center gap-2 ${
                  isSel ? "bg-sky-50 text-sky-700" : "text-gray-700 hover:bg-gray-50"
                }`}
              >
                <span className="truncate flex-1">
                  {d.source === "mic" ? "🎤 " : "🔊 "}
                  {d.name}
                  {d.is_default ? t("source.default") : ""}
                </span>
                {isSel && <Check className="w-3.5 h-3.5 shrink-0" />}
              </button>
            );
          })}

          {/* Refresh lives inside the list — no separate button outside. */}
          <div className="mt-1 pt-1 border-t border-gray-100">
            <button
              onClick={refresh}
              disabled={loading}
              className="w-full text-left rounded-md px-3 py-2 text-sm text-gray-500 hover:bg-gray-50 hover:text-gray-700 cursor-pointer transition-colors flex items-center gap-2 disabled:opacity-50"
            >
              <RotateCw className={`w-3.5 h-3.5 shrink-0 ${loading ? "animate-spin" : ""}`} />
              {t("source.refreshList")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

// ============================================================================
// OS input volume — the real Windows endpoint volume (audio_volume.rs), the
// same slider as Settings → System → Sound → Input → Volume.
// ============================================================================

function VolumeSlider({ source }: { source: SelectedSource }) {
  const t = useT();
  const [vol, setVol] = useState<number | null>(null);
  const [unsupported, setUnsupported] = useState(false);

  // (Re)load the selected source's OS volume whenever it changes.
  useEffect(() => {
    let alive = true;
    audioApi
      .getSourceVolume(source.name, source.source)
      .then((v) => {
        if (alive) {
          setVol(v);
          setUnsupported(false);
        }
      })
      .catch(() => {
        if (alive) setUnsupported(true);
      });
    return () => {
      alive = false;
    };
  }, [source.name, source.source]);

  // Non-Windows (or no reachable endpoint): hide the slider rather than show a
  // dead control.
  if (unsupported) return null;

  const pct = Math.round((vol ?? 0) * 100);
  const onChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const v = Number(e.target.value) / 100;
    setVol(v);
    // Push to the OS immediately — the slider IS the system volume.
    audioApi.setSourceVolume(source.name, source.source, v).catch(() => {});
  };

  return (
    <div className="mt-3 flex items-center gap-3">
      <Volume2 className="w-4 h-4 text-gray-400 shrink-0" />
      <input
        type="range"
        min={0}
        max={100}
        step={1}
        value={pct}
        onChange={onChange}
        disabled={vol === null}
        aria-label={t("source.volume")}
        className="flex-1 accent-sky-600 cursor-pointer disabled:opacity-50"
      />
      <span className="text-xs text-gray-500 tabular-nums w-9 text-right">{pct}%</span>
    </div>
  );
}

// ============================================================================
// Input level meter — live input loudness vs a "적정" target zone, so the user
// can set a good volume without playing the recording back. Thresholds are on
// the normalized FFT-peak scale (empirical).
// ============================================================================

const LEVEL_LOW = 0.18;
const LEVEL_HIGH = 0.55;

function LevelMeter({ level, recording }: { level: number; recording: boolean }) {
  const t = useT();
  const pct = Math.min(1, level) * 100;
  const tooLow = level < LEVEL_LOW;
  const tooHigh = level > LEVEL_HIGH;
  const status: DictKey | null = !recording
    ? null
    : tooLow
      ? "source.level.low"
      : tooHigh
        ? "source.level.high"
        : "source.level.ok";
  const fillColor = tooLow ? "bg-gray-300" : tooHigh ? "bg-amber-400" : "bg-emerald-400";
  const statusColor =
    status === "source.level.ok"
      ? "text-emerald-600"
      : status === "source.level.high"
        ? "text-amber-600"
        : "text-gray-400";

  return (
    <div>
      <div className="flex items-center justify-between mb-1">
        <span className={SUBLABEL_CLS}>{t("source.level")}</span>
        {status && <span className={`text-xs font-medium ${statusColor}`}>{t(status)}</span>}
      </div>
      <div className="relative h-2.5 rounded-full bg-gray-100 overflow-hidden">
        <div
          className={`h-full ${fillColor} transition-[width] duration-75`}
          style={{ width: `${pct}%` }}
        />
        {/* 적정 구간 경계 — 이 두 눈금 사이가 권장 입력 레벨. */}
        <div
          className="absolute inset-y-0 w-px bg-gray-400/60"
          style={{ left: `${LEVEL_LOW * 100}%` }}
        />
        <div
          className="absolute inset-y-0 w-px bg-gray-400/60"
          style={{ left: `${LEVEL_HIGH * 100}%` }}
        />
      </div>
      <p className="mt-1 text-[11px] text-gray-400">{t("source.level.hint")}</p>
    </div>
  );
}

// ============================================================================
// Input test — waveform + live level meter + record/playback.
// ============================================================================

function InputTestBody({ source }: { source: SelectedSource }) {
  const t = useT();
  const [testId, setTestId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [playbackUrl, setPlaybackUrl] = useState<string | null>(null);
  const [level, setLevel] = useState(0);
  const playbackUrlRef = useRef<string | null>(null);
  const testIdRef = useRef<string | null>(null);
  const levelTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    playbackUrlRef.current = playbackUrl;
  }, [playbackUrl]);
  testIdRef.current = testId;
  useEffect(
    () => () => {
      if (playbackUrlRef.current) URL.revokeObjectURL(playbackUrlRef.current);
      if (testIdRef.current) audioApi.stopTestCapture(testIdRef.current).catch(() => {});
      if (levelTimer.current) clearTimeout(levelTimer.current);
    },
    [],
  );

  // Stop a running test when the selected source changes out from under us.
  const srcKey = `${source.source}:${source.name}`;
  useEffect(() => {
    if (testIdRef.current) {
      audioApi.stopTestCapture(testIdRef.current).catch(() => {});
      setTestId(null);
    }
    if (playbackUrlRef.current) {
      URL.revokeObjectURL(playbackUrlRef.current);
      setPlaybackUrl(null);
    }
    setLevel(0);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [srcKey]);

  const clearPlayback = () => {
    if (playbackUrl) URL.revokeObjectURL(playbackUrl);
    setPlaybackUrl(null);
  };

  const start = async () => {
    clearPlayback();
    setBusy(true);
    try {
      setTestId(await audioApi.startTestCapture(source.name, source.source));
    } catch (e) {
      toast.error(t("source.test.startFail", { error: String(e) }));
    } finally {
      setBusy(false);
    }
  };

  const stop = async () => {
    if (!testId) return;
    setBusy(true);
    try {
      const bytes = await audioApi.stopTestCapture(testId);
      const blob = new Blob([new Uint8Array(bytes)], { type: "audio/webm" });
      setPlaybackUrl(URL.createObjectURL(blob));
    } catch (e) {
      toast.error(t("source.test.stopFail", { error: String(e) }));
    } finally {
      setTestId(null);
      setLevel(0);
      setBusy(false);
    }
  };

  const recording = !!testId;

  // Live input level → meter. Decay to 0 shortly after sound stops so the meter
  // falls back instead of freezing on the last peak.
  const onLevel = (peak: number) => {
    setLevel(peak);
    if (levelTimer.current) clearTimeout(levelTimer.current);
    levelTimer.current = window.setTimeout(() => setLevel(0), 250);
  };

  return (
    <>
      <p className={`${SUBLABEL_CLS} mb-1`}>{t("source.waveform")}</p>
      {/* Centred ~70% so the bars aren't edge-to-edge; soft tinted box makes the
          waveform area legible at a glance. */}
      <div className="mb-4 w-[70%] mx-auto bg-gray-50 rounded-xl py-1.5 px-8">
        <WaveBars
          targetId={testId ?? ""}
          fill
          gap={2}
          maxHeight={56}
          color="bg-sky-400"
          onLevel={recording ? onLevel : undefined}
        />
      </div>

      <div className="mb-10">
        <LevelMeter level={recording ? level : 0} recording={recording} />
      </div>

      {recording ? (
        <button
          onClick={stop}
          disabled={busy}
          className="w-full flex items-center justify-center gap-2 px-5 py-2.5 bg-gray-800 text-white rounded-lg text-sm font-medium hover:bg-gray-900 transition-colors disabled:opacity-60 border-0 cursor-pointer"
        >
          {busy ? <Loader2 className="w-4 h-4 animate-spin" /> : <Square className="w-3.5 h-3.5" />}
          {t("source.test.stop")}
        </button>
      ) : (
        <button
          onClick={start}
          disabled={busy}
          className="w-full flex items-center justify-center gap-2 px-5 py-2.5 bg-sky-600 text-white rounded-lg text-sm font-medium hover:bg-sky-700 transition-colors disabled:opacity-60 border-0 cursor-pointer shadow-sm"
        >
          {busy ? <Loader2 className="w-4 h-4 animate-spin" /> : <Play className="w-3.5 h-3.5" />}
          {t("source.test.start")}
        </button>
      )}

      {playbackUrl && !recording && (
        <>
          <hr className="my-5 border-gray-100" />
          <p className={CAPTION_CLS}>{t("source.playback")}</p>
          <audio src={playbackUrl} controls className="w-full h-9" />
        </>
      )}
    </>
  );
}
