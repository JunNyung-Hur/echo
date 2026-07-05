// 노트 본문 테마 프리셋 — 본문은 순수 마크다운, 디자인은 여기 CSS가 담당한다.
// (마크다운 전환: LLM이 문서마다 CSS를 생성하던 구조를 폐기하고, 앱이 테마를
// 소유한다. str_replace 편집·제목 파생·diff가 전부 내용만 보게 하기 위함.)
// id는 notes.theme 컬럼 값과 1:1 (백엔드 repo/notes.rs THEME_IDS와 동기).

export interface ThemePreset {
  id: string;
  nameKo: string;
  nameEn: string;
  /** 프리뷰 스와치(선택 UI 점 색). */
  swatch: string;
  css: string;
}

const BASE = `
  * { box-sizing: border-box; }
  html, body { margin: 0; }
  body { word-break: break-word; -webkit-font-smoothing: antialiased; }
  img { max-width: 100%; }
  a { color: #2563eb; }
`;

/** 미니멀 화이트 — 기존 minutes 기본 디자인 계승. */
const DEFAULT_CSS = `${BASE}
  body {
    font-family: -apple-system, 'Pretendard', 'Noto Sans KR', sans-serif;
    line-height: 1.75;
    color: #1a1a1a;
    max-width: 800px;
    margin: 0 auto;
    padding: 32px 44px 64px;
    font-size: 15px;
    background: #fff;
  }
  h1 { font-size: 22px; font-weight: 700; margin: 0 0 4px; color: #111; }
  h1 + p { font-size: 14px; color: #666; margin: 0 0 32px; padding-bottom: 16px; border-bottom: 1px solid #e5e5e5; }
  h2 { font-size: 17px; font-weight: 700; color: #111; margin: 28px 0 12px; padding-bottom: 6px; border-bottom: 1px solid #e5e5e5; }
  h3 { font-size: 15px; font-weight: 600; color: #111; margin: 20px 0 8px; }
  p { margin: 0 0 12px; }
  ul, ol { margin: 0 0 16px; padding-left: 20px; }
  li { margin-bottom: 4px; color: #333; }
  li > ul, li > ol { margin-bottom: 0; }
  strong { font-weight: 700; }
  hr { border: 0; border-top: 1px solid #e5e5e5; margin: 24px 0; }
  blockquote { margin: 0 0 16px; padding: 4px 14px; border-left: 3px solid #d1d5db; color: #555; }
  code { background: #f3f4f6; padding: 1px 5px; border-radius: 4px; font-size: 13px; }
  pre { background: #f8f9fa; border: 1px solid #e5e7eb; border-radius: 6px; padding: 12px; overflow-x: auto; }
  pre code { background: transparent; padding: 0; }
  table { border-collapse: collapse; margin: 0 0 16px; }
  th, td { border: 1px solid #e5e7eb; padding: 5px 12px; font-size: 14px; }
  th { background: #f9fafb; font-weight: 600; }
`;

/** 노란 괘선 공책 — 기존 freeform 템플릿 이식(줄간격 32px에 글이 앉는다). */
const NOTEPAD_CSS = `${BASE}
  body {
    min-height: 100%;
    padding: 4px 48px 64px;
    background-color: #fffdf2;
    background-image: repeating-linear-gradient(#fffdf2, #fffdf2 31px, #e7dfbf 31px, #e7dfbf 32px);
    background-position: 0 4px;
    font-family: 'Pretendard', -apple-system, BlinkMacSystemFont, system-ui, sans-serif;
    font-size: 15px;
    line-height: 32px;
    color: #2d2a20;
  }
  p { margin: 0; line-height: 32px; }
  ul, ol { margin: 0; padding-left: 22px; }
  li { line-height: 32px; }
  h1 { font-size: 22px; line-height: 32px; margin: 32px 0 0; font-weight: 700; }
  h2 { font-size: 19px; line-height: 32px; margin: 32px 0 0; font-weight: 700; }
  h3 { font-size: 16px; line-height: 32px; margin: 32px 0 0; font-weight: 600; }
  body > :first-child { margin-top: 0; }
  strong { font-weight: 700; }
  hr { border: 0; border-top: 2px dashed #d9d2b0; margin: 15px 0 16px; }
  blockquote { margin: 0; padding-left: 14px; border-left: 3px solid #d9d2b0; color: #6b6450; }
  code { background: rgba(0,0,0,0.05); padding: 0 5px; border-radius: 4px; font-size: 13px; }
  pre { background: rgba(0,0,0,0.04); border-radius: 6px; padding: 0 12px; overflow-x: auto; }
  table { border-collapse: collapse; margin: 0; }
  td, th { border: 1px solid #d9d2b0; padding: 3px 10px; line-height: 26px; }
  th { background: rgba(0,0,0,0.03); }
`;

/** 보고서 — 세리프 제목 + 네이비 라인, 문서 결재 느낌. */
const REPORT_CSS = `${BASE}
  body {
    font-family: 'Pretendard', 'Noto Sans KR', -apple-system, sans-serif;
    line-height: 1.8;
    color: #1f2430;
    max-width: 760px;
    margin: 0 auto;
    padding: 36px 44px 64px;
    font-size: 15px;
    background: #fff;
  }
  h1 {
    font-family: 'Noto Serif KR', Georgia, serif;
    font-size: 24px; font-weight: 700; color: #14213d;
    margin: 0 0 6px; padding-bottom: 12px; border-bottom: 3px double #14213d;
    text-align: center;
  }
  h1 + p { text-align: center; font-size: 13px; color: #667085; margin: 8px 0 36px; }
  h2 {
    font-size: 16px; font-weight: 700; color: #14213d;
    margin: 30px 0 12px; padding: 6px 10px;
    background: #f1f4f9; border-left: 4px solid #14213d;
  }
  h3 { font-size: 15px; font-weight: 600; color: #14213d; margin: 20px 0 8px; }
  p { margin: 0 0 12px; }
  ul, ol { margin: 0 0 16px; padding-left: 22px; }
  li { margin-bottom: 5px; }
  li::marker { color: #14213d; }
  strong { font-weight: 700; color: #14213d; }
  hr { border: 0; border-top: 1px solid #cbd2dc; margin: 26px 0; }
  blockquote { margin: 0 0 16px; padding: 6px 14px; border-left: 3px solid #94a3b8; background: #f8fafc; color: #475569; }
  code { background: #eef2f7; padding: 1px 5px; border-radius: 3px; font-size: 13px; }
  pre { background: #f8fafc; border: 1px solid #dbe2ea; border-radius: 4px; padding: 12px; overflow-x: auto; }
  table { border-collapse: collapse; margin: 0 0 16px; width: 100%; }
  th, td { border: 1px solid #cbd2dc; padding: 6px 12px; font-size: 14px; }
  th { background: #14213d; color: #fff; font-weight: 600; }
`;

/** 컬러풀 — 밝은 그라디언트 헤더와 파스텔 섹션. */
const COLORFUL_CSS = `${BASE}
  body {
    font-family: 'Pretendard', -apple-system, system-ui, sans-serif;
    line-height: 1.8;
    color: #27272a;
    max-width: 800px;
    margin: 0 auto;
    padding: 32px 44px 64px;
    font-size: 15px;
    background: #fffcfa;
  }
  h1 {
    font-size: 24px; font-weight: 800; margin: 0 0 6px;
    background: linear-gradient(90deg, #f97316, #ec4899, #8b5cf6);
    -webkit-background-clip: text; background-clip: text; color: transparent;
  }
  h1 + p { font-size: 13px; color: #a1a1aa; margin: 0 0 30px; }
  h2 {
    font-size: 17px; font-weight: 700; color: #7c3aed;
    margin: 28px 0 12px; padding: 5px 12px;
    background: #f5f3ff; border-radius: 8px;
  }
  h2:nth-of-type(3n+2) { color: #db2777; background: #fdf2f8; }
  h2:nth-of-type(3n) { color: #ea580c; background: #fff7ed; }
  h3 { font-size: 15px; font-weight: 700; color: #6d28d9; margin: 20px 0 8px; }
  p { margin: 0 0 12px; }
  ul, ol { margin: 0 0 16px; padding-left: 22px; }
  li { margin-bottom: 5px; }
  li::marker { color: #ec4899; }
  strong { font-weight: 700; color: #be185d; }
  hr { border: 0; border-top: 2px dashed #fbcfe8; margin: 24px 0; }
  blockquote { margin: 0 0 16px; padding: 6px 14px; border-left: 4px solid #f9a8d4; background: #fdf2f8; border-radius: 0 8px 8px 0; }
  code { background: #ede9fe; color: #6d28d9; padding: 1px 6px; border-radius: 6px; font-size: 13px; }
  pre { background: #faf5ff; border: 1px solid #e9d5ff; border-radius: 10px; padding: 12px; overflow-x: auto; }
  table { border-collapse: separate; border-spacing: 0; margin: 0 0 16px; border: 1px solid #e9d5ff; border-radius: 10px; overflow: hidden; }
  th, td { border-bottom: 1px solid #f3e8ff; padding: 6px 12px; font-size: 14px; }
  tr:last-child td { border-bottom: 0; }
  th { background: linear-gradient(90deg, #f5f3ff, #fdf2f8); color: #7c3aed; font-weight: 700; }
`;

export const THEMES: ThemePreset[] = [
  { id: "default", nameKo: "미니멀", nameEn: "Minimal", swatch: "#e5e7eb", css: DEFAULT_CSS },
  { id: "notepad", nameKo: "노트패드", nameEn: "Notepad", swatch: "#f4e9b8", css: NOTEPAD_CSS },
  { id: "report", nameKo: "보고서", nameEn: "Report", swatch: "#14213d", css: REPORT_CSS },
  { id: "colorful", nameKo: "컬러풀", nameEn: "Colorful", swatch: "#ec4899", css: COLORFUL_CSS },
];

export function themeById(id: string | null | undefined): ThemePreset {
  return THEMES.find((t) => t.id === id) ?? THEMES[0];
}
