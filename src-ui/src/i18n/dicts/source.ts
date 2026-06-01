import type { Entry } from "../dict";

/**
 * source 단어장 — 입력 소스 선택/설정 패널(SourceSelector): 노트 Step1 모달 + 설정 오디오 탭.
 */
export const source = {
  "source.empty": {
    ko: "캡처 가능한 입력 소스가 없어요. 마이크를 연결한 뒤 다시 검색해주세요.",
    en: "No capturable input source. Connect a mic and search again.",
  },
  "source.refresh": { ko: "다시 검색", en: "Search again" },
  "source.refreshList": { ko: "장치 다시 검색", en: "Re-scan devices" },
  "source.settings": { ko: "입력 소스 설정", en: "Input source settings" },
  "source.select": { ko: "입력 소스 선택", en: "Select input source" },
  "source.selectFirst": { ko: "먼저 입력 소스를 선택해주세요.", en: "Select an input source first." },
  "source.test": { ko: "입력 소스 테스트", en: "Test input source" },
  "source.default": { ko: " (기본)", en: " (default)" },
  "source.volume": { ko: "입력 소스 볼륨", en: "Input volume" },

  // level meter
  "source.level": { ko: "입력 레벨", en: "Input level" },
  "source.level.low": { ko: "작음", en: "Low" },
  "source.level.high": { ko: "큼", en: "High" },
  "source.level.ok": { ko: "적정", en: "Good" },
  "source.level.hint": {
    ko: "말했을 때 막대가 두 눈금 사이(적정)에 닿도록 볼륨을 맞춰보세요.",
    en: "Speak and adjust the volume so the bar lands between the two marks (the good zone).",
  },

  "source.waveform": { ko: "입력 파형", en: "Waveform" },
  "source.playback": { ko: "녹음 재생", en: "Playback" },
  "source.test.start": { ko: "테스트 시작", en: "Start test" },
  "source.test.stop": { ko: "테스트 종료", en: "Stop test" },
  "source.test.startFail": { ko: "테스트 시작 실패: {error}", en: "Failed to start test: {error}" },
  "source.test.stopFail": { ko: "테스트 종료 실패: {error}", en: "Failed to stop test: {error}" },
} as const satisfies Record<string, Entry>;
