import { invoke } from "@tauri-apps/api/core";
import type { Recording } from "@/api/recordings";

// 순서 있는 응답 parts — 한 전송에 대한 assistant 응답을 [text/tool/ask] 블록으로
// 표현 (Meetzy parts 모델 이식). 백엔드가 JSON 배열 문자열로 저장한다.
export interface ChatTextPart {
  type: "text";
  text: string;
}
export interface ChatToolPart {
  type: "tool";
  tool_id?: string;
  name: string;
  args?: unknown;
  status?: "running" | "completed" | "failed";
  elapsed_s?: number | null;
  result?: {
    ok?: boolean;
    error?: string;
    file_path?: string;
    filename?: string;
    transcript_id?: string;
    preview?: string;
    theme?: string;
    /** edit_minutes 의 블록 단위 변경 — 카드 밑 빨강/초록 diff UI 로 렌더. */
    diffs?: Array<{ before?: string; after?: string }>;
    [k: string]: unknown;
  } | null;
  minutes_version_id?: string | null;
}
/** ask_user 질문 카드 — 에이전트가 질문하고 턴을 멈춘 지점. options 비면 네/아니오. */
export interface ChatAskPart {
  type: "ask";
  question: string;
  options?: string[];
}
export type ChatPart = ChatTextPart | ChatToolPart | ChatAskPart;

export interface ChatMessage {
  id: string;
  note_id: string;
  role: string; // "user" | "assistant"
  content: string;
  note_body_version_id: string | null;
  tool_calls: string | null; // JSON [{id,name,args,result}] (레거시)
  parts: string | null; // JSON ChatPart[] — 발생 순서 보존
  created_at: string;
  /** Step 4: recordings this user message sent — rendered as bubble chips. */
  recordings: Recording[];
}

/** parts JSON 파싱 (없거나 깨지면 null → content 폴백 렌더). */
export function parseParts(m: ChatMessage): ChatPart[] | null {
  if (!m.parts) return null;
  try {
    const v = JSON.parse(m.parts);
    return Array.isArray(v) ? (v as ChatPart[]) : null;
  } catch {
    return null;
  }
}

export const chatApi = {
  list: (noteId: string) => invoke<ChatMessage[]>("list_chat_messages", { noteId }),
  /** Runs the agent loop (may take 1-2min if it refines). Resolves when done. */
  send: (noteId: string, message: string, userState?: unknown) =>
    invoke<void>("chat_send", { noteId, message, userState }),
  /** 이 노트의 에이전트 턴이 진행 중인가 — 재진입 시 pending 인디케이터 복원용. */
  running: (noteId: string) => invoke<boolean>("chat_running", { noteId }),
};
