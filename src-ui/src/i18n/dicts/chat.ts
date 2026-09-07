import type { Entry } from "../dict";

/**
 * chat 단어장 — 채팅 패널(ChatPanel): 도구 진행 라벨·헤더·상태·입력.
 * 전사 언어 라벨(kor/eng/auto)·시각미정은 detail.lang.* / detail.time.unset 재사용.
 */
export const chat = {
  "chat.tool.searchTranscripts": { ko: "녹음 근거 찾는 중…", en: "Finding source evidence…" },
  "chat.step.searchTranscripts": { ko: "녹음 근거 검색", en: "Search source evidence" },
  // tool progress (TOOL_LABEL — 진행 라벨)
  "chat.tool.readMinutes": { ko: "노트 확인 중…", en: "Reading the note…" },
  "chat.tool.edit": { ko: "노트 편집 중…", en: "Editing the note…" },
  "chat.tool.setTheme": { ko: "테마 변경 중…", en: "Changing the theme…" },
  "chat.tool.writeNote": { ko: "노트에 옮기는 중…", en: "Writing to the note…" },
  "chat.tool.recordingUrl": { ko: "녹음 파일 확인 중…", en: "Checking the recording…" },
  "chat.tool.readTranscript": { ko: "전사록 읽는 중…", en: "Reading the transcript…" },
  "chat.tool.retryTranscribe": { ko: "전사 다시 시작 중…", en: "Restarting transcription…" },
  "chat.tool.retryTask": { ko: "작업 재시작 중…", en: "Restarting the task…" },

  // tool step cards (완료형 라벨 — parts 카드)
  "chat.step.readMinutes": { ko: "노트 확인", en: "Read note" },
  "chat.step.edit": { ko: "노트 편집", en: "Edit note" },
  "chat.step.setTheme": { ko: "테마 변경", en: "Change theme" },
  "chat.step.writeNote": { ko: "노트 작성", en: "Write note" },
  "chat.step.recordingUrl": { ko: "녹음 파일", en: "Recording file" },
  "chat.step.readTranscript": { ko: "전사록 조회", en: "Read transcript" },
  "chat.step.transcribeAttachment": { ko: "녹음 전사", en: "Transcribe recording" },
  "chat.step.retryTranscribe": { ko: "전사 재시작", en: "Restart transcription" },
  "chat.step.retryTask": { ko: "작업 재시작", en: "Restart task" },
  "chat.tool.failed": { ko: "실패했어요", en: "Failed" },
  "chat.tool.noDetail": { ko: "(상세 없음)", en: "(no detail)" },
  "chat.tool.secSuffix": { ko: "초", en: "s" },
  "chat.tool.openFile": { ko: "파일 열기", en: "Open file" },

  // ask_user 질문 카드
  "chat.ask.yes": { ko: "네", en: "Yes" },
  "chat.ask.no": { ko: "아니오", en: "No" },
  "chat.ask.direct": { ko: "직접 입력", en: "Type my own" },
  "chat.ask.directHint": { ko: "답변을 직접 입력하세요", en: "Type your answer" },

  // transcribing short labels (TRANSCRIBING_STEP_LABEL — 입력창 위 진행 카드)
  "chat.transcribing.finalize": { ko: "오디오 정리 중", en: "Cleaning up audio" },
  "chat.transcribing.transcribe": { ko: "전사 중", en: "Transcribing" },
  "chat.transcribing.minutes": { ko: "노트 만드는 중", en: "Making note" },

  // status
  "chat.thinking": { ko: "생각하는 중…", en: "Thinking…" },
  "chat.processing": { ko: "처리 중…", en: "Working…" },

  // header
  "chat.back": { ko: "노트 목록으로", en: "Back to notes" },
  "chat.noLocation": { ko: "장소 없음", en: "No location" },
  "chat.more": { ko: "더보기", en: "More" },
  "chat.delete": { ko: "노트 삭제", en: "Delete note" },

  // version chip
  "chat.viewVersion": { ko: "이 시점 본문 보기", en: "View this version" },
  "chat.viewVersion.title": {
    ko: "이 시점 본문 보기 · 비교 / 되돌리기",
    en: "View this version · compare / restore",
  },

  // scroll / recording / failure
  "chat.scrollLatest": { ko: "최신 메시지로 이동", en: "Jump to latest" },
  "chat.recording": { ko: "녹음 중", en: "Recording" },
  "chat.fail.minutes": { ko: "본문 생성 실패", en: "Body generation failed" },
  "chat.fail.transcribe": { ko: "전사 실패", en: "Transcription failed" },
  "chat.retry": { ko: "다시 시도", en: "Retry" },

  // input
  "chat.input.placeholder": { ko: "메시지를 입력하세요", en: "Type a message" },
  "chat.send": { ko: "보내기", en: "Send" },

  // recording attach (Step 3)
  "chat.rec.record": { ko: "녹음 첨부", en: "Attach recording" },
  "chat.rec.start": { ko: "녹음 시작", en: "Start recording" },
  "chat.rec.stop": { ko: "중지", en: "Stop" },
  "chat.rec.finalizing": { ko: "녹음 정리 중…", en: "Finishing up…" },
  "chat.rec.attached": { ko: "녹음 첨부됨", en: "Recording attached" },
  "chat.rec.cancel": { ko: "첨부 취소", en: "Remove attachment" },
  "chat.rec.noSource": { ko: "입력 소스를 먼저 선택하세요", en: "Pick an input source first" },
  "chat.rec.finalizeTimeout": { ko: "녹음 정리에 실패했습니다", en: "Failed to finish the recording" },
  "chat.rec.attachedOnly": { ko: "🎤 녹음", en: "🎤 Recording" },
  "chat.rec.leaveTitle": { ko: "녹음 중이에요", en: "Recording in progress" },
  "chat.rec.leaveDesc": {
    ko: "지금 나가면 진행 중인 녹음이 취소됩니다. 나가시겠어요?",
    en: "Leaving now cancels the recording in progress. Leave anyway?",
  },
  "chat.rec.leaveStay": { ko: "계속 녹음", en: "Keep recording" },
  "chat.rec.leaveLeave": { ko: "나가기", en: "Leave" },
  "chat.rec.noSound": { ko: "소리가 감지되지 않아요", en: "No sound detected" },
  "chat.rec.play": { ko: "재생", en: "Play" },
  "chat.rec.pause": { ko: "정지", en: "Stop" },
  "chat.rec.transcribing": { ko: "녹음 전사 중…", en: "Transcribing…" },
  "chat.rec.drafting": { ko: "녹음 정리 중…", en: "Drafting…" },
  "chat.rec.merging": { ko: "노트에 통합 중…", en: "Merging into the note…" },
  "chat.rec.upload": { ko: "파일 첨부", en: "Attach file" },
  "chat.rec.dropHint": { ko: "오디오 파일을 여기에 놓으세요", en: "Drop an audio file here" },
  "chat.rec.archive": { ko: "보관함", en: "Archive" },
  "chat.rec.archiveTitle": { ko: "파일 보관함", en: "File archive" },
  "chat.rec.openFolder": { ko: "폴더 열기", en: "Open folder" },
  "chat.rec.delTitle": { ko: "녹음을 삭제할까요?", en: "Delete this recording?" },
  "chat.rec.delDesc": {
    ko: "이 녹음 파일이 완전히 삭제되고 되돌릴 수 없습니다. 삭제하시겠어요?",
    en: "This recording will be permanently deleted and can't be undone. Delete it?",
  },
  "chat.rec.delCancel": { ko: "취소", en: "Cancel" },
  "chat.rec.delConfirm": { ko: "삭제", en: "Delete" },
} as const satisfies Record<string, Entry>;
