import { invoke } from "@tauri-apps/api/core";

export type EndpointKind = "llm" | "asr";

export interface AiEndpoint {
  id: string;
  kind: EndpointKind;
  name: string;
  model_id: string;
  api_base_url: string;
  api_key: string;
  /** eb0b667 — "chat_completions" | "transcriptions". Was audio_format. */
  request_mode: string;
  chunk_seconds: number | null;
  max_tokens: number | null;
  is_active: number;
  created_at: string;
  updated_at: string;
}

export interface CreateEndpointInput {
  kind: EndpointKind;
  name: string;
  model_id: string;
  api_base_url: string;
  api_key?: string;
  request_mode?: string;
  chunk_seconds?: number | null;
  max_tokens?: number | null;
}

export interface UpdateEndpointInput {
  name?: string;
  model_id?: string;
  api_base_url?: string;
  api_key?: string;
  request_mode?: string;
  chunk_seconds?: number | null;
  max_tokens?: number | null;
}

export interface TestResult {
  success: boolean;
  message: string;
  response_time_ms: number | null;
}

export const endpointsApi = {
  list: (kind?: EndpointKind) => invoke<AiEndpoint[]>("list_endpoints", { kind: kind ?? null }),
  create: (input: CreateEndpointInput) => invoke<AiEndpoint>("create_endpoint", { input }),
  update: (id: string, input: UpdateEndpointInput) =>
    invoke<AiEndpoint>("update_endpoint", { id, input }),
  delete: (id: string) => invoke<void>("delete_endpoint", { id }),
  activate: (id: string) => invoke<AiEndpoint>("activate_endpoint", { id }),
  test: (id: string) => invoke<TestResult>("test_endpoint", { id }),
};
