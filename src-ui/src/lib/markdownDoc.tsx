// 마크다운 본문 → iframe srcDoc 용 완결 HTML 문서.
//
// 본문은 순수 마크다운으로 저장되고(마크다운 전환), 렌더 시점에 테마 CSS를 입혀
// iframe에 넣는다 — 기존 본문 뷰어의 높이 조절·복사(서식) 로직이 iframe
// contentDocument를 읽는 구조라 그 기계를 그대로 재사용한다. 레거시 본문
// (full HTML 문서 / freeform HTML 조각)은 isHtmlContent로 구분해 종전 경로로.

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

import { themeById } from "@/lib/themes";

/** '<'로 시작하면 레거시 HTML(full doc 또는 freeform 조각). 백엔드 body_ext_for와 동일 판별. */
export function isHtmlContent(content: string): boolean {
  return content.trimStart().startsWith("<");
}

/** 마크다운 → HTML 문자열 (GFM). */
export function renderMarkdownHtml(md: string): string {
  return renderToStaticMarkup(
    createElement(ReactMarkdown, { remarkPlugins: [remarkGfm] }, md),
  );
}

/** 마크다운 본문 + 테마 → iframe srcDoc 완결 문서. */
export function buildThemedDoc(md: string, themeId: string | null | undefined): string {
  const theme = themeById(themeId);
  const body = renderMarkdownHtml(md);
  return `<!DOCTYPE html><html><head><meta charset="utf-8"><style>${theme.css}</style></head><body>${body}</body></html>`;
}

/** HTML *조각*(freeform 레거시) + 테마 → srcDoc. full doc이 와도 body 안쪽만 추출해 감싼다. */
export function buildThemedFragmentDoc(fragment: string, themeId: string | null | undefined): string {
  const theme = themeById(themeId);
  const m = fragment.match(/<body[^>]*>([\s\S]*?)<\/body>/i);
  const inner = m ? m[1] : fragment;
  return `<!DOCTYPE html><html><head><meta charset="utf-8"><style>${theme.css}</style></head><body>${inner}</body></html>`;
}

/**
 * 본문 종류별 iframe srcDoc 결정 — 뷰어(DonePanel)와 버전 이력이 공유.
 * - full HTML 문서(레거시 minutes: 자체 <style> 보유) → 그대로
 * - HTML 조각(레거시 freeform) → 테마 CSS로 감쌈
 * - 마크다운(신규) → 렌더 + 테마 CSS
 */
export function srcDocFor(content: string, themeId: string | null | undefined): string {
  const t = content.trimStart().toLowerCase();
  if (t.startsWith("<!doctype") || t.startsWith("<html")) return content;
  if (t.startsWith("<")) return buildThemedFragmentDoc(content, themeId);
  return buildThemedDoc(content, themeId);
}
