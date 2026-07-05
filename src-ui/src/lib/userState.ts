/**
 * User-visible state derivation — Meetzy lib/userState.ts (3-E-1) 이식.
 *
 * 매 채팅 전송에 동봉하는 `user_state` 스냅샷. capability registry가 핵심 —
 * 사용자가 *지금 이 화면에서* 볼 수 있는/할 수 있는/할 수 없는 모든 행동을
 * 열거하고, AI가 수행 가능한 행동(`ai_tool != null`)과 사용자 전용 행동
 * (`ai_tool == null`, UI에서만 가능)을 구분한다. LLM은 이걸로 도구를 부를지
 * 화면 위치를 안내할지 판단하고, 백엔드는 `hidden` 항목의 ai_tool을
 * 도구 목록에서 제거한다(capability gating).
 */

export type Stage = "before" | "recording" | "transcribing" | "done" | "freeform";

export interface CapabilityAction {
  state: "enabled" | "disabled" | "hidden";
  ai_tool: string | null;
  user_ui_location: string | null;
  disabled_reason: string | null;
}

export interface CapabilityInputs {
  stage: Stage;
  hasFinalizedRecording: boolean; // 정리된(webm) 녹음 존재
  hasActiveBody: boolean; // 활성 완료 본문 존재
  hasFailedTranscript: boolean;
  hasFailedBody: boolean;
  archivedBodyCount: number;
}

// 사용자에게 그대로 안내되는 문구 — 내부 컴포넌트 이름은 사용자에게 의미 없는
// 코드 단어라 사용 금지. 화면 위치를 사용자 시각으로 가리키는 표현으로.
const UI = {
  recordStart: "노트 상세 화면 우측 가운데의 빨간 '녹음 시작' 버튼",
  recordStop: "녹음 중 화면 가운데의 '녹음 중지' 버튼",
  uploadAudio: "노트 상세 우측 화면 아래쪽의 오디오 파일 업로드 영역",
  editManual: "노트 패널 우상단의 '수정' 버튼",
  viewVersionHistory: "노트 패널 우상단의 '변경 이력' 버튼",
  restoreVersion: "변경 이력 모달 우측 하단의 '되돌리기' 버튼",
  themePicker: "노트 패널 우상단의 '테마' 버튼",
  transcriptView: "노트 패널 우상단의 '전사록' 버튼",
  deleteNote: "좌측 chat 헤더 우측 '···' 메뉴의 '노트 삭제'",
  retryButton: "우측 패널의 '다시 시도' 버튼",
  archive: "좌측 chat 헤더의 '보관함' 버튼",
} as const;

function action(
  state: CapabilityAction["state"],
  ai_tool: string | null,
  user_ui_location: string | null,
  disabled_reason?: string,
): CapabilityAction {
  return { state, ai_tool, user_ui_location, disabled_reason: disabled_reason ?? null };
}

/** 현재 화면의 capability registry (Meetzy buildCapabilityRegistry 이식, echo 도메인). */
export function buildCapabilityRegistry(c: CapabilityInputs): Record<string, CapabilityAction> {
  const reg: Record<string, CapabilityAction> = {};
  const bodyEditable = (c.stage === "done" || c.stage === "freeform") && c.hasActiveBody;

  // 노트 편집(AI 직접 수행) — 활성 본문이 있을 때만.
  reg.edit_minutes = bodyEditable
    ? action("enabled", "edit_minutes", null)
    : action("hidden", null, null);

  // freeform 받아적기 — 필기형 전용.
  reg.write_note =
    c.stage === "freeform" ? action("enabled", "write_note", null) : action("hidden", null, null);

  // 테마 전환 — freeform(노트 필기형) 전용. 회의록 작성형은 고정 기본 테마.
  reg.set_theme =
    c.stage === "freeform"
      ? action("enabled", "set_theme", UI.themePicker)
      : action("hidden", null, null);

  // --- User-direct actions (AI는 못 함) ---
  reg.edit_minutes_manual =
    c.stage === "done" || c.stage === "freeform"
      ? action("enabled", null, UI.editManual)
      : action("hidden", null, null);

  reg.view_version_history =
    c.archivedBodyCount >= 1 && c.hasActiveBody
      ? action("enabled", null, UI.viewVersionHistory)
      : action("hidden", null, null);

  reg.restore_minutes_version =
    c.archivedBodyCount >= 1
      ? action("enabled", null, UI.restoreVersion)
      : action("hidden", null, null);

  // --- Recording lifecycle (minutes형) ---
  reg.record_start =
    c.stage === "before" ? action("enabled", null, UI.recordStart) : action("hidden", null, null);
  reg.record_stop =
    c.stage === "recording" ? action("enabled", null, UI.recordStop) : action("hidden", null, null);
  reg.upload_audio =
    c.stage === "before" ? action("enabled", null, UI.uploadAudio) : action("hidden", null, null);

  // --- 녹음 파일 전달 (로컬 파일 버튼) ---
  reg.download_recording = c.hasFinalizedRecording
    ? action("enabled", "get_recording_download_url", UI.archive)
    : action("hidden", null, null);

  // --- Retry — 실패한 작업이 있을 때만 ---
  reg.retry_failed_task =
    c.hasFailedTranscript || c.hasFailedBody
      ? action("enabled", "retry_failed_task", UI.retryButton)
      : action("hidden", null, null);

  // retry_transcribe: 파괴적(노트까지 폐기)이라 UI 표면 없음, chat 전용.
  reg.retry_transcribe =
    (c.stage === "transcribing" || c.stage === "done") && c.hasFinalizedRecording
      ? action("enabled", "retry_transcribe", null)
      : action("hidden", null, null);

  // read_transcript: chat 전용. done에서만(활성 본문 = 완료 전사 경유).
  reg.read_transcript =
    c.stage === "done" && c.hasActiveBody
      ? action("enabled", "read_transcript", UI.transcriptView)
      : action("hidden", null, null);

  // --- Always-on ---
  reg.delete_note = action("enabled", null, UI.deleteNote);

  return reg;
}

export interface BuildUserStateInputs extends CapabilityInputs {
  versionHistoryOpen: boolean;
  transcribingFailureKind: "transcript" | "minutes" | null;
  /** freeform 첨부 전송 녹음 id들 — 백엔드 첨부 파이프라인 트리거. */
  recordingIds?: string[];
}

export function buildUserState(i: BuildUserStateInputs): Record<string, unknown> {
  const state: Record<string, unknown> = {
    stage: i.stage,
    version_history_open: i.versionHistoryOpen,
    available_actions: buildCapabilityRegistry(i),
    transcribing_failure: i.transcribingFailureKind
      ? { kind: i.transcribingFailureKind, retryable: true }
      : null,
  };
  if (i.recordingIds && i.recordingIds.length > 0) {
    state.recordingIds = i.recordingIds;
  }
  return state;
}
