import { invoke } from "@tauri-apps/api/core";
import type { Recording } from "@/api/recordings";

export interface ChatMessage {
  id: string;
  note_id: string;
  role: string; // "user" | "assistant"
  content: string;
  note_body_version_id: string | null;
  tool_calls: string | null; // JSON [{id,name,args,result}]
  created_at: string;
  /** Step 4: recordings this user message sent — rendered as bubble chips. */
  recordings: Recording[];
}

export const chatApi = {
  list: (noteId: string) => invoke<ChatMessage[]>("list_chat_messages", { noteId }),
  /** Runs the agent loop (may take 1-2min if it refines). Resolves when done. */
  send: (noteId: string, message: string, userState?: unknown) =>
    invoke<void>("chat_send", { noteId, message, userState }),
};
