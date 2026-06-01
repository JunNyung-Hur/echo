// Selected capture source — persisted in localStorage (machine-local).
// With native cpal capture (D-023) a source is identified by {name, source}
// where source is "mic" (real input) or "system" (output endpoint, loopback).

export interface SelectedSource {
  name: string;
  source: "mic" | "system";
}

const SOURCE_KEY = "echo.audio.source";

export function getSavedSource(): SelectedSource | null {
  try {
    const raw = localStorage.getItem(SOURCE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed.name === "string" && (parsed.source === "mic" || parsed.source === "system")) {
      return parsed;
    }
    return null;
  } catch {
    return null;
  }
}

export function saveSource(s: SelectedSource): void {
  try {
    localStorage.setItem(SOURCE_KEY, JSON.stringify(s));
  } catch {
    /* ignore */
  }
}
