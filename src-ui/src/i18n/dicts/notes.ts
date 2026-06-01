import type { Entry } from "../dict";

/**
 * notes 단어장 — 홈(NotesPage): 액션·검색·기간 필터·목록·빈 상태.
 * 인사말/날짜 포맷은 lib/datetime 의 lang 분기에서 다룬다(여기 아님).
 */
export const notes = {
  "notes.loading": { ko: "불러오는 중…", en: "Loading…" },
  "notes.settings": { ko: "설정", en: "Settings" },

  // empty (no notes at all)
  "notes.empty.title": { ko: "첫 생각을 캡처해볼까요?", en: "Capture your first thought?" },
  "notes.empty.desc": {
    ko: "녹음·메모·강의 무엇이든 던져두면\nAI 가 정리·검색·연결해드려요.",
    en: "Drop in a recording, memo, or lecture —\nAI organizes, searches, and connects it for you.",
  },
  "notes.empty.create": { ko: "새 노트 만들기", en: "Create your first note" },

  // action row
  "notes.new": { ko: "새 노트", en: "New note" },
  "notes.search.placeholder": {
    ko: "제목, 메모, 장소로 검색  (⌘K)",
    en: "Search by title, memo, location  (⌘K)",
  },

  // date filter
  "notes.dateFilter": { ko: "기간", en: "Date" },
  "notes.dateFilter.clear": { ko: "기간 필터 해제", en: "Clear date filter" },
  "notes.dateFilter.from": { ko: "시작", en: "From" },
  "notes.dateFilter.to": { ko: "종료", en: "To" },
  "notes.dateFilter.hint": {
    ko: "한 칸만 채우면 그 날짜만 필터. 두 칸 모두 채우면 기간(양 끝 포함).",
    en: "Fill one box for a single day; fill both for a range (inclusive).",
  },
  "notes.dateFilter.reset": { ko: "지우기", en: "Clear" },

  // list
  "notes.empty.filtered": { ko: "조건에 맞는 노트가 없어요.", en: "No notes match your filters." },
  "notes.search.empty": { ko: "검색 결과가 없어요.", en: "No results found." },
  "notes.more": { ko: "더 보기", en: "Show more" },
  "notes.status.processing": { ko: "처리 중", en: "Processing" },
  "notes.status.ready": { ko: "준비됨", en: "Ready" },

  // date groups
  "notes.group.today": { ko: "오늘", en: "Today" },
  "notes.group.yesterday": { ko: "어제", en: "Yesterday" },
} as const satisfies Record<string, Entry>;
