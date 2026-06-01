-- Step 3/4 멀티 녹음 첨부 회복(recovery).
-- consumed_at NULL = 전송 대기(첨부 칩으로 복원 대상),
-- 값 있음        = 전송·소비 완료(칩에서 빠지고 노트 녹음 이력으로 이동).
-- 진입 시 consumed_at IS NULL & finalized_at IS NOT NULL 인 녹음을 칩으로 되살린다.
ALTER TABLE recordings ADD COLUMN consumed_at TEXT;
