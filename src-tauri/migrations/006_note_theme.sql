-- 006: 노트별 테마 (마크다운 전환 — 디자인을 본문에서 분리, 앱 프리셋 테마가 담당).
-- 본문은 순수 마크다운(신규)이고 프리셋 CSS는 프론트가 소유한다. theme 값은
-- 프리셋 id 문자열 (default / notepad / report / colorful).
ALTER TABLE notes ADD COLUMN theme TEXT NOT NULL DEFAULT 'default';

-- 기존 freeform 노트는 노란 괘선 공책 템플릿으로 렌더되고 있었음 → notepad 테마로 이전.
UPDATE notes SET theme = 'notepad' WHERE note_type = 'freeform';
