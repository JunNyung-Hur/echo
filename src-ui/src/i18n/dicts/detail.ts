import type { Entry } from "../dict";

/**
 * detail 단어장 — 노트 상세(NoteDetailPage): 녹음 시작·전사 진행/실패·본문 패널.
 * 채팅(ChatPanel)·입력 소스(SourceSelector)는 각자 컴포넌트에서 따로 다룬다.
 */
export const detail = {
  // toasts
  "detail.toast.selectSource": {
    ko: "입력 소스를 먼저 선택해주세요 (설정 또는 ⚙).",
    en: "Select an input source first (Settings or ⚙).",
  },
  "detail.toast.recordStartFail": {
    ko: "녹음 시작 실패: {error}",
    en: "Failed to start recording: {error}",
  },
  "detail.toast.recordStopFail": {
    ko: "녹음 정지 실패: {error}",
    en: "Failed to stop recording: {error}",
  },
  "detail.toast.retranscribe": { ko: "전사를 다시 시작했어요", en: "Transcription restarted" },
  "detail.toast.importFail": {
    ko: "파일 가져오기 실패: {error}",
    en: "Failed to import file: {error}",
  },
  "detail.toast.deleted": { ko: "노트가 삭제됐어요", en: "Note deleted" },
  "detail.toast.saved": { ko: "저장했어요", en: "Saved" },
  "detail.delete.confirm": {
    ko: "이 노트를 삭제할까요? 연결된 녹음·전사·본문도 함께 삭제됩니다.",
    en: "Delete this note? Its recordings, transcripts, and body will be deleted too.",
  },

  // empty / record-start
  "detail.notFound": { ko: "노트를 찾을 수 없어요.", en: "Note not found." },
  "detail.record.title": { ko: "녹음을 시작해주세요", en: "Start a recording" },
  "detail.title.placeholder": { ko: "노트 제목", en: "Note title" },
  "detail.location.placeholder": { ko: "장소", en: "Location" },
  "detail.location.input": { ko: "장소 입력", en: "Add location" },
  "detail.record.start": { ko: "녹음 시작", en: "Start recording" },
  "detail.or": { ko: "또는", en: "or" },
  "detail.time.unset": { ko: "시각 미정", en: "Time not set" },

  // transcription language options
  "detail.lang.auto": { ko: "자동", en: "Auto" },
  "detail.lang.kor": { ko: "한국어", en: "Korean" },
  "detail.lang.eng": { ko: "영어", en: "English" },

  // transcribing steps + headings
  "detail.step.finalize": { ko: "오디오 정리", en: "Audio cleanup" },
  "detail.step.transcribe": { ko: "전사 (음성 → 텍스트)", en: "Transcription (speech → text)" },
  "detail.step.minutes": { ko: "노트 정리", en: "Organizing note" },
  "detail.heading.finalize": { ko: "오디오를 정리하고 있어요", en: "Cleaning up the audio…" },
  "detail.heading.transcribe": { ko: "내용을 받아 적고 있어요", en: "Transcribing your audio…" },
  "detail.heading.minutes": { ko: "노트를 정리하고 있어요", en: "Organizing your note…" },
  "detail.transcribing.desc": {
    ko: "전사가 끝나면 자동으로 정리해요. 다른 노트로 이동하셔도 안전합니다.",
    en: "I'll organize it automatically after transcription. You can safely switch to another note.",
  },
  "detail.progress": { ko: "진행률", en: "Progress" },

  // transcribing failure / recovery
  "detail.fail.minutes.title": {
    ko: "본문 생성 중 문제가 발생했어요",
    en: "Something went wrong generating the body",
  },
  "detail.fail.transcribe.title": {
    ko: "전사 중 문제가 발생했어요",
    en: "Something went wrong during transcription",
  },
  "detail.fail.minutes.desc": {
    ko: "AI 모델 서버에 문제가 발생하여 본문 생성을 완료하지 못했습니다.\n잠시 후 다시 시도해주세요.",
    en: "The AI model server had a problem and couldn't finish generating the body.\nPlease try again shortly.",
  },
  "detail.fail.transcribe.desc": {
    ko: "AI 모델 서버에 문제가 발생하여 전사를 완료하지 못했습니다.\n잠시 후 다시 시도해주세요.",
    en: "The AI model server had a problem and couldn't finish transcription.\nPlease try again shortly.",
  },
  "detail.retry": { ko: "다시 시도", en: "Try again" },
  "detail.recordDone.title": { ko: "녹음 정리 완료", en: "Recording ready" },
  "detail.recordDone.desc": {
    ko: "전사를 시작하면 본문을 자동으로 정리해드려요.",
    en: "Start transcription and I'll organize the body for you.",
  },
  "detail.transcribe.start": { ko: "전사 시작", en: "Start transcription" },

  // body panel + meta
  "detail.meta.title": { ko: "제목", en: "Title" },
  "detail.meta.location": { ko: "장소", en: "Location" },
  "detail.meta.language": { ko: "언어", en: "Language" },
  "detail.meta.startedAt": { ko: "시작 시각", en: "Start time" },
  "detail.save": { ko: "저장", en: "Save" },
  "detail.saving": { ko: "저장 중…", en: "Saving…" },
  "detail.history": { ko: "변경 이력", en: "History" },
  "detail.transcript": { ko: "전사록", en: "Transcript" },
  "detail.editManual": { ko: "직접 수정", en: "Edit manually" },
  "detail.edit": { ko: "수정", en: "Edit" },
  "detail.metaChanged.prefix": { ko: "본문 생성 이후 ", en: "After the body was generated, " },
  "detail.metaChanged.suffix": {
    ko: " 정보가 바뀌었어요. 본문에 반영하려면 채팅으로 요청하세요.",
    en: " changed. Ask in chat to reflect it in the body.",
  },
  "detail.copy": { ko: "복사", en: "Copy" },
  "detail.copy.text": { ko: "텍스트 복사", en: "Copy as text" },
  "detail.copy.formatted": { ko: "서식 복사", en: "Copy formatted" },
  "detail.body.edit": { ko: "본문 편집", en: "Edit body" },
  "detail.body.title": { ko: "노트 본문", en: "Note body" },
  "detail.body.loadFail": { ko: "본문을 불러올 수 없어요.", en: "Couldn't load the body." },
} as const satisfies Record<string, Entry>;
