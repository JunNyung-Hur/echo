import type { Entry } from "../dict";

/**
 * common 단어장 — 여러 화면에서 공유하는 버튼·동작 라벨.
 */
export const common = {
  "common.save": { ko: "저장", en: "Save" },
  "common.cancel": { ko: "취소", en: "Cancel" },
  "common.confirm": { ko: "확인", en: "OK" },
  "common.delete": { ko: "삭제", en: "Delete" },
  "common.edit": { ko: "수정", en: "Edit" },
  "common.close": { ko: "닫기", en: "Close" },
  "common.retry": { ko: "다시 시도", en: "Retry" },
} as const satisfies Record<string, Entry>;
