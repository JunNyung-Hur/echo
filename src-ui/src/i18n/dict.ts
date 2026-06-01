/**
 * KO/EN 번역 단어장 (i18n) — aggregator.
 *
 * 라이브러리 없이 직접 운용. 각 키는 { ko, en } 한 쌍이고 UI는 useT()의
 * t(key) 로 참조한다. 보간은 `{name}` 플레이스홀더 + t(key, { name }).
 * 키 네이밍: 도메인.용도 (common.*, settings.*, note.*, ...).
 *
 * 도메인별 단어는 dicts/*.ts 모듈로 나뉘고 여기서 합친다(스프레드).
 * 새 모듈 추가 시: import 후 아래 dict 스프레드에 한 줄 추가.
 *
 * 번역하지 않는 것: 회의록(본문) / 채팅 어시스턴트 응답 / tool 프롬프트 —
 * LLM 파이프라인이 ui_lang 에 따라 생성·소비(별도 backend 경로).
 */

import { common } from "./dicts/common";
import { notes } from "./dicts/notes";
import { detail } from "./dicts/detail";
import { settings } from "./dicts/settings";
import { chat } from "./dicts/chat";
import { source } from "./dicts/source";
import { tags } from "./dicts/tags";
import { misc } from "./dicts/misc";

export type Lang = "ko" | "en";

export interface Entry {
  ko: string;
  en: string;
}

export const dict = {
  ...common,
  ...notes,
  ...detail,
  ...settings,
  ...chat,
  ...source,
  ...tags,
  ...misc,
} satisfies Record<string, Entry>;

export type DictKey = keyof typeof dict;
