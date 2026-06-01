import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

/** Rust emits this many spectrum bars (audio_capture.rs WAVE_BARS); we resample
 *  down to the requested `barCount`. */
const SOURCE_BARS = 80;
/** Lerp toward each incoming frame (Rust already EMA-smooths, so keep this light). */
const SMOOTH = 0.85;

interface Props {
  /** recording_id (or test_id) whose `recording:level` events to render. */
  targetId: string;
  /** Bars to render (resampled from the 80-bar source). Recording widget uses
   *  80 (dense, like the old design); the input test uses ~32. Ignored when
   *  `fill` is set. */
  barCount?: number;
  /** Max bar height in px. */
  maxHeight?: number;
  /** Idle/flat bar height in px (old recording widget = 6, input test = 3). */
  minHeight?: number;
  /** Gap between bars in px (old recording widget = 3, input test = 2). */
  gap?: number;
  color?: string;
  /** Called with the peak spectrum level (0–1) on each incoming frame — drives
   *  the "sound detected" indicator. Not called during idle decay. */
  onLevel?: (peak: number) => void;
  /** Fill the container width: bar count is derived from the measured width so
   *  the waveform stays dense at any width (narrow note modal ↔ wide settings
   *  tab) instead of clumping in the centre. Overrides `barCount`. */
  fill?: boolean;
}

/** Average-resample a source array to `n` bins. */
function resample(src: number[], n: number): number[] {
  if (src.length === n) return src.slice();
  const out = new Array(n).fill(0);
  for (let i = 0; i < n; i++) {
    const a = Math.floor((i * src.length) / n);
    const b = Math.max(a + 1, Math.floor(((i + 1) * src.length) / n));
    let s = 0;
    let c = 0;
    for (let k = a; k < b && k < src.length; k++) {
      s += src[k];
      c++;
    }
    out[i] = c > 0 ? s / c : 0;
  }
  return out;
}

/**
 * Centre-symmetric live waveform driven by Rust `recording:level` events. The
 * Rust side sends an FFT magnitude spectrum (SOURCE_BARS bins); we resample to
 * the bar count and lerp toward it. Packed + centred to match the old design.
 */
export default function WaveBars({
  targetId,
  barCount = SOURCE_BARS,
  maxHeight = 96,
  minHeight = 3,
  gap = 3,
  color = "bg-red-400",
  onLevel,
  fill = false,
}: Props) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const [autoCount, setAutoCount] = useState(barCount);
  const count = fill ? autoCount : barCount;
  const [bars, setBars] = useState<number[]>(() => new Array(count).fill(0));
  const targetRef = useRef(targetId);
  targetRef.current = targetId;
  const onLevelRef = useRef(onLevel);
  onLevelRef.current = onLevel;

  // Derive bar count from the container width when `fill` is on, so the bars
  // span the full width at any size. Each bar is w-1 (4px) + `gap`.
  useEffect(() => {
    if (!fill) return;
    const el = wrapRef.current;
    if (!el) return;
    const measure = () => {
      const per = 4 + gap;
      setAutoCount(Math.max(8, Math.floor(el.clientWidth / per)));
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [fill, gap]);

  // Reset bar buffer when the count changes.
  useEffect(() => {
    setBars(new Array(count).fill(0));
  }, [count]);

  useEffect(() => {
    const unlisten = listen<{ recording_id: string; bars: number[] }>(
      "recording:level",
      (e) => {
        if (e.payload.recording_id !== targetRef.current) return;
        const incoming = e.payload.bars;
        if (!incoming?.length) return;
        const resampled = resample(incoming, count);
        let peak = 0;
        for (const v of resampled) if (v > peak) peak = v;
        onLevelRef.current?.(peak);
        setBars((prev) =>
          prev.map((p, i) => p + ((resampled[i] ?? 0) - p) * SMOOTH),
        );
      },
    );
    // Decay to flat when no events arrive (e.g. silence / stopped).
    const decay = window.setInterval(() => {
      setBars((prev) => prev.map((p) => (p > 0.01 ? p * 0.8 : 0)));
    }, 120);
    return () => {
      unlisten.then((fn) => fn());
      clearInterval(decay);
    };
  }, [count]);

  return (
    <div
      ref={wrapRef}
      className="w-full flex items-center justify-center"
      style={{ height: maxHeight, gap: `${gap}px` }}
      aria-hidden
    >
      {bars.map((lvl, i) => (
        <div
          key={i}
          className={`w-1 rounded-full self-center transition-[height] duration-75 ${color}`}
          style={{ height: `${Math.max(minHeight, Math.min(1, lvl) * maxHeight)}px` }}
        />
      ))}
    </div>
  );
}
