import type { Entry } from "../dict";

/**
 * settings 단어장 — 설정 페이지(SettingsPage): 탭·언어 섹션·AI 모델 폼.
 * 오디오 탭은 SourceSelector 가 자체 처리.
 */
export const settings = {
  "settings.title": { ko: "설정", en: "Settings" },
  "settings.back": { ko: "노트 목록", en: "Notes" },
  "settings.tab.audio": { ko: "오디오", en: "Audio" },
  "settings.tab.models": { ko: "AI 모델", en: "AI Models" },
  "settings.tab.language": { ko: "언어", en: "Language" },

  // language section
  "settings.lang.title": { ko: "표시 언어", en: "Display language" },
  "settings.lang.desc": {
    ko: "앱 화면과 AI가 정리하는 노트·채팅·타임라인에 사용할 언어입니다.",
    en: "Language for the app UI and the AI-generated notes, chat, and timeline.",
  },

  // AI models
  "settings.models.intro": {
    ko: "OpenAI 호환 endpoint 를 등록하세요. 외부 API 든 로컬 서버(Ollama 등)든 같은 방식입니다.",
    en: "Register an OpenAI-compatible endpoint — an external API or a local server (Ollama, etc.) works the same way.",
  },
  "settings.kind.llm": { ko: "LLM (요약·정리·채팅)", en: "LLM (summary · cleanup · chat)" },
  "settings.kind.asr": { ko: "ASR (음성 인식)", en: "ASR (speech recognition)" },
  "settings.empty": { ko: "등록된 {kind} endpoint 가 없어요.", en: "No {kind} endpoint registered yet." },
  "settings.toast.updated": { ko: "수정됐어요", en: "Updated" },
  "settings.toast.added": { ko: "추가됐어요", en: "Added" },
  "settings.delete.confirm": { ko: "이 endpoint 를 삭제할까요?", en: "Delete this endpoint?" },
  "settings.model.inUse": { ko: "사용 중인 모델", en: "Model in use" },
  "settings.model.clickUse": { ko: "클릭해서 이 모델 사용", en: "Click to use this model" },
  "settings.model.active": { ko: "사용 중", en: "In use" },
  "settings.model.clickActivate": { ko: "클릭해서 사용", en: "Click to use" },
  "settings.action.test": { ko: "연결 테스트", en: "Test connection" },
  "settings.action.edit": { ko: "수정", en: "Edit" },
  "settings.action.delete": { ko: "삭제", en: "Delete" },

  // form
  "settings.field.name": { ko: "이름", en: "Name" },
  "settings.field.name.ph.llm": { ko: "예: GPT-4o mini", en: "e.g. GPT-4o mini" },
  "settings.field.name.ph.asr": { ko: "예: Whisper", en: "e.g. Whisper" },
  "settings.field.modelId": { ko: "모델 ID", en: "Model ID" },
  "settings.apiKey.note": {
    ko: "로컬 SQLite 에 평문 저장됩니다 (1인 앱). 공유 PC 에서는 주의하세요.",
    en: "Stored as plaintext in the local SQLite DB (single-user app). Be careful on shared PCs.",
  },
  "settings.field.asrMode": { ko: "ASR 추론 방식", en: "ASR request mode" },
  "settings.asrMode.chat": {
    ko: "v1/chat/completions — audio_url 입력 (Qwen3-ASR, VibeVoice 등)",
    en: "v1/chat/completions — audio_url input (Qwen3-ASR, VibeVoice, etc.)",
  },
  "settings.asrMode.transcribe": {
    ko: "v1/audio/transcriptions — 파일 업로드 (whisper-1, GPT-4o Transcribe, Voxtral 등)",
    en: "v1/audio/transcriptions — file upload (whisper-1, GPT-4o Transcribe, Voxtral, etc.)",
  },
  "settings.field.chunk": { ko: "청크 길이 (초)", en: "Chunk length (sec)" },
  "settings.field.chunk.ph": { ko: "기본 300 (5분)", en: "Default 300 (5 min)" },
  "settings.field.maxTokens": { ko: "최대 출력 토큰", en: "Max output tokens" },
  "settings.field.maxTokens.ph": { ko: "기본 4096", en: "Default 4096" },
  "settings.form.update": { ko: "수정", en: "Update" },
  "settings.form.add": { ko: "추가", en: "Add" },
  "settings.form.cancel": { ko: "취소", en: "Cancel" },
  "settings.add": { ko: "{kind} endpoint 추가", en: "Add {kind} endpoint" },
} as const satisfies Record<string, Entry>;
