/**
 * 언어 상태 Context — lang('ko'|'en') · setLang · t() 제공.
 *
 * 단일 소스 = settings 테이블(ui_lang). backend(회의록·채팅·timeline 출력 언어)도
 * 같은 값을 읽으므로 DB 가 진실원본이다. localStorage 는 첫 렌더 플리커를 막기
 * 위한 *동기* 초기값 미러일 뿐 — mount 직후 settings 를 읽어 확정/보정한다.
 * - 초기값: localStorage 동기 read (없으면 'ko').
 * - mount: settings.get('ui_lang') → 있으면 반영(+ 미러 갱신). 사용자가 이미
 *   토글했으면(explicit) 덮어쓰지 않는다.
 * - 토글: state 즉시 갱신 + localStorage 미러 + settings.set 영속.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { dict, type DictKey, type Lang } from "./dict";
import { settingsApi } from "@/api/settings";

const LANG_KEY = "echo.ui_lang";

function initialLang(): Lang {
  try {
    const s = localStorage.getItem(LANG_KEY);
    if (s === "ko" || s === "en") return s;
  } catch {
    /* ignore */
  }
  return "ko";
}

function translate(
  lang: Lang,
  key: DictKey,
  params?: Record<string, string | number>,
): string {
  const entry = dict[key];
  // 키가 누락되면(아직 배선 안 된 문구) 키 자체를 노출 — 빌드 타입체크로
  // 대부분 잡히지만 런타임 안전망.
  let s: string = entry ? entry[lang] : (key as string);
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      s = s.split(`{${k}}`).join(String(v));
    }
  }
  return s;
}

export type TFunc = (
  key: DictKey,
  params?: Record<string, string | number>,
) => string;

interface LangContextValue {
  lang: Lang;
  setLang: (l: Lang) => void;
  t: TFunc;
}

const LangContext = createContext<LangContextValue | null>(null);

export function LangProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(initialLang);
  // 사용자가 이번 세션에 직접 토글했는지 — 토글 후엔 늦게 도착한 settings.get
  // 결과가 되돌리지 않게 한다(레이스 방지).
  const explicit = useRef(false);

  useEffect(() => {
    settingsApi
      .get("ui_lang")
      .then((v) => {
        if (explicit.current) return;
        if (v === "ko" || v === "en") {
          setLangState(v);
          try {
            localStorage.setItem(LANG_KEY, v);
          } catch {
            /* ignore */
          }
        }
      })
      .catch(() => {
        /* ignore — localStorage/기본값 유지 */
      });
  }, []);

  const setLang = useCallback((l: Lang) => {
    explicit.current = true;
    setLangState(l);
    try {
      localStorage.setItem(LANG_KEY, l);
    } catch {
      /* ignore */
    }
    settingsApi.set("ui_lang", l).catch(() => {
      /* ignore — 다음 토글 때 다시 시도됨 */
    });
  }, []);

  const t = useCallback<TFunc>(
    (key, params) => translate(lang, key, params),
    [lang],
  );

  return (
    <LangContext.Provider value={{ lang, setLang, t }}>
      {children}
    </LangContext.Provider>
  );
}

export function useLang(): LangContextValue {
  const ctx = useContext(LangContext);
  if (!ctx) throw new Error("useLang must be used within LangProvider");
  return ctx;
}

export function useT(): TFunc {
  return useLang().t;
}

/**
 * 선택 가능한 UI 언어 목록 — 언어 추가 시 ① 여기 한 줄 ② dict 의 Lang 유니온
 * ③ dicts/* 에 해당 언어 값. label 은 그 언어의 자기 표기(endonym).
 */
export const LANGS: { code: Lang; label: string }[] = [
  { code: "ko", label: "한국어" },
  { code: "en", label: "English" },
];
