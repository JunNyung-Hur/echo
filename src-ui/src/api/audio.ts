import { invoke } from "@tauri-apps/api/core";

export interface AudioDeviceInfo {
  name: string;
  /** "mic" = real input, "system" = output endpoint via loopback. */
  source: "mic" | "system";
  is_default: boolean;
  capturable: boolean;
}

export const audioApi = {
  listDevices: () => invoke<AudioDeviceInfo[]>("list_audio_devices"),
  /** Start a test capture; emits `recording:level` waveform keyed by returned id. */
  startTestCapture: (name: string, source: "mic" | "system") =>
    invoke<string>("start_test_capture", { name, source }),
  /** Stop test capture; returns recorded webm bytes for playback. */
  stopTestCapture: (testId: string) => invoke<number[]>("stop_test_capture", { testId }),
  /** OS volume scalar (0.0–1.0) of the source, via Windows Core Audio. */
  getSourceVolume: (name: string, source: "mic" | "system") =>
    invoke<number>("get_source_volume", { name, source }),
  /** Set the source's OS volume scalar (0.0–1.0). */
  setSourceVolume: (name: string, source: "mic" | "system", level: number) =>
    invoke<void>("set_source_volume", { name, source, level }),
};
