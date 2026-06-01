import type { Entry } from "../dict";

/**
 * misc 단어장 — 작은 하위 컴포넌트들: 녹음 위젯·버전 이력·파일 업로더·스피너·마크다운.
 */
export const misc = {
  // RecordingWidget
  "rec.recording": { ko: "녹음 중", en: "Recording" },
  "rec.stop": { ko: "녹음 중지", en: "Stop recording" },
  "rec.noSound": {
    ko: "소리가 감지되지 않고 있어요. 입력 소스나 볼륨을 확인해주세요.",
    en: "No sound is being detected. Check your input source or volume.",
  },
  "rec.chunkError": { ko: "청크 저장에 문제가 있어요: {error}", en: "Chunk save problem: {error}" },

  // VersionHistory
  "version.loadFail": { ko: "변경 이력을 불러오지 못했습니다", en: "Couldn't load the history" },
  "version.restored": { ko: "이 버전으로 되돌렸어요", en: "Restored this version" },
  "version.title": { ko: "변경 이력", en: "History" },
  "version.list": { ko: "정리 이력", en: "Versions" },
  "version.loading": { ko: "불러오는 중…", en: "Loading…" },
  "version.empty": { ko: "완료된 버전이 없습니다.", en: "No completed versions." },
  "version.current": { ko: "현재", en: "Current" },
  "version.manual": { ko: "직접 수정", en: "Manual edit" },
  "version.bodyLoading": { ko: "본문 불러오는 중…", en: "Loading body…" },
  "version.bodyEmpty": { ko: "본문이 비어 있습니다.", en: "The body is empty." },
  "version.currentNote": { ko: "현재 적용된 본문입니다.", en: "This is the current body." },
  "version.restoreNote": {
    ko: "되돌리면 현재 본문은 이력에 보존되고, 이 버전이 새 active가 됩니다.",
    en: "Restoring keeps the current body in history and makes this version the active one.",
  },
  "version.currentBody": { ko: "현재 본문", en: "Current body" },
  "version.restoring": { ko: "되돌리는 중…", en: "Restoring…" },
  "version.restore": { ko: "이 버전으로 되돌리기", en: "Restore this version" },

  // FileUploader
  "uploader.audio": { ko: "오디오", en: "Audio" },
  "uploader.importing": { ko: "가져오는 중…", en: "Importing…" },
  "uploader.dropHint": {
    ko: "오디오 파일을 드래그하거나 클릭하여 선택",
    en: "Drag an audio file here, or click to choose",
  },

  // Spinner
  "spinner.loading": { ko: "로딩 중", en: "Loading" },

  // AssistantMarkdown
  "md.copy": { ko: "복사", en: "Copy" },
  "md.copied": { ko: "복사됨", en: "Copied" },

  // TranscriptViewerModal
  "transcript.loadFail": { ko: "전사록을 불러오지 못했습니다.", en: "Couldn't load the transcript." },
  "transcript.title": { ko: "전사록 원문", en: "Transcript" },
  "transcript.loading": { ko: "전사록을 불러오는 중…", en: "Loading transcript…" },
  "transcript.empty": { ko: "(빈 전사록)", en: "(empty transcript)" },
  "transcript.viewFull": { ko: "전체 전사록 보기", en: "View full transcript" },
  "transcript.viewFullShort": { ko: "전체보기", en: "View all" },
} as const satisfies Record<string, Entry>;
