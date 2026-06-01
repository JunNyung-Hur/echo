import { invoke } from "@tauri-apps/api/core";

/** App-wide KV settings (single-user). Currently the UI language (`ui_lang`). */
export const settingsApi = {
  /** Read a setting, or null if never set. */
  get: (key: string) => invoke<string | null>("get_setting", { key }),
  /** Upsert a setting. */
  set: (key: string, value: string) => invoke<void>("set_setting", { key, value }),
};
