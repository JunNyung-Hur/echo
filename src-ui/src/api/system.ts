import { invoke } from "@tauri-apps/api/core";

export const systemApi = {
  getAppVersion: () => invoke<string>("get_app_version"),
  pingDb: () => invoke<string>("ping_db"),
  getUsername: () => invoke<string>("get_username"),
};
