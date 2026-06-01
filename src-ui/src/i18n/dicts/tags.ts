import type { Entry } from "../dict";

/**
 * tags 단어장 — 태그 칩/자동완성(노트 헤더) + 태그 필터(홈).
 */
export const tags = {
  "tags.add": { ko: "태그 추가", en: "Add tag" },
  "tags.addPlaceholder": { ko: "", en: "" },
  "tags.create": { ko: "‘{name}’ 새 태그", en: "New tag ‘{name}’" },
  "tags.remove": { ko: "태그 제거", en: "Remove tag" },
  "tags.filterHint": { ko: "태그로 필터", en: "Filter by tag" },
} as const satisfies Record<string, Entry>;
