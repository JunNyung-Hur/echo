-- 003 — 노트 유형(회의록 작성형 minutes / 노트 필기형 freeform). 선택 후 고정.
-- 신규 노트는 NULL(미선택)로 시작해 진입 시 유형 선택 UI를 띄운다.
-- 기존 노트는 전부 회의록형으로 소급(회귀 방지).
ALTER TABLE notes ADD COLUMN note_type TEXT;
UPDATE notes SET note_type = 'minutes';
