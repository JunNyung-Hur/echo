import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { AlertTriangle, WifiOff } from "lucide-react";

import WaveBars from "@/components/WaveBars";
import { useT } from "@/i18n/LangContext";

interface Props {
  recordingId: string;
  elapsed: number;
  onStop: () => void;
}

function formatElapsed(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const m = Math.floor(total / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(m)}:${pad(s)}`;
}

/**
 * Native-capture recording widget (D-023). Waveform via the shared WaveBars
 * (smoothed, centre-symmetric, driven by Rust `recording:level`). Silence /
 * chunk-error warnings come from the level/chunk_error events.
 */
export default function RecordingWidget({ recordingId, elapsed, onStop }: Props) {
  const t = useT();
  const [chunkError, setChunkError] = useState<string | null>(null);
  const [showSilence, setShowSilence] = useState(false);
  const lastSoundRef = useRef<number>(Date.now());

  useEffect(() => {
    const unlistenLevel = listen<{ recording_id: string; bars: number[] }>(
      "recording:level",
      (e) => {
        if (e.payload.recording_id !== recordingId) return;
        if (e.payload.bars?.some((b) => b > 0.04)) lastSoundRef.current = Date.now();
      },
    );
    const unlistenErr = listen<{ recording_id: string; message: string }>(
      "recording:chunk_error",
      (e) => {
        if (e.payload.recording_id !== recordingId) return;
        setChunkError(e.payload.message);
      },
    );
    const silenceTimer = window.setInterval(() => {
      setShowSilence(Date.now() - lastSoundRef.current > 3000);
    }, 500);
    return () => {
      unlistenLevel.then((fn) => fn());
      unlistenErr.then((fn) => fn());
      clearInterval(silenceTimer);
    };
  }, [recordingId]);

  return (
    <div className="flex flex-col h-full min-h-0 items-center justify-center px-8 py-12">
      <div className="w-full max-w-2xl flex flex-col items-center gap-10">
        <div className="text-center">
          <div className="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-red-50 border border-red-100 mb-3">
            <span className="w-2 h-2 bg-red-500 rounded-full animate-pulse" />
            <span className="text-xs font-medium text-red-700">{t("rec.recording")}</span>
          </div>
          <div className="text-5xl font-mono font-medium text-gray-900 tabular-nums">
            {formatElapsed(elapsed)}
          </div>
        </div>

        <WaveBars targetId={recordingId} barCount={80} maxHeight={112} minHeight={6} color="bg-red-400" />

        <button
          onClick={onStop}
          className="px-6 py-3 bg-gray-800 hover:bg-gray-900 text-white rounded-full text-sm font-medium flex items-center gap-2 transition-colors cursor-pointer border-0 shadow-sm"
        >
          <span className="w-2.5 h-2.5 bg-red-500 rounded-sm" />
          {t("rec.stop")}
        </button>

        {showSilence && !chunkError && (
          <div className="flex items-center gap-2 px-3 py-2 bg-amber-50 border border-amber-200 rounded-md max-w-md">
            <AlertTriangle className="w-4 h-4 text-amber-500 shrink-0" />
            <span className="text-sm text-amber-700">{t("rec.noSound")}</span>
          </div>
        )}

        {chunkError && (
          <div className="flex items-center gap-2 px-3 py-2 bg-red-50 border border-red-200 rounded-md max-w-md">
            <WifiOff className="w-4 h-4 text-red-500 shrink-0" />
            <span className="text-sm text-red-700">{t("rec.chunkError", { error: chunkError })}</span>
          </div>
        )}
      </div>
    </div>
  );
}
