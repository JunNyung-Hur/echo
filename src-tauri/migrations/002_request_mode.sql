-- 002 — eb0b667: audio_format → request_mode (값도 엔드포인트 이름으로 정렬:
-- audio_url → chat_completions, openai_transcribe → transcriptions) + LLM 전사
-- 후처리(postprocess) 제거. 전사록은 ASR raw 출력 그대로 저장 (모델 성능·인프라
-- 문제로 오래 미사용이던 normalizer 정리).

ALTER TABLE ai_endpoints ADD COLUMN request_mode TEXT NOT NULL DEFAULT 'chat_completions';

UPDATE ai_endpoints
   SET request_mode = CASE
       WHEN audio_format = 'openai_transcribe' THEN 'transcriptions'
       ELSE 'chat_completions'
   END;

ALTER TABLE ai_endpoints DROP COLUMN audio_format;
ALTER TABLE ai_endpoints DROP COLUMN postprocess_enabled;
