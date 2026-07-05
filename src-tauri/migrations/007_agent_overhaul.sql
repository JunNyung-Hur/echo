-- 007: 에이전트 개편 (Meetzy d75150c 싱크)
--  - note_chat_messages.parts: 한 전송에 대한 assistant 응답을 [text/tool/ask]
--    블록의 발생 순서 JSON 배열로 저장. 히스토리 직렬화가 이 순서를 복원해
--    "완료 보고가 호출보다 먼저"를 모델이 학습하는 루프를 차단한다.
--  - note_bodies.refine_request: 이 버전을 만든 사용자 요청(편집 근거) —
--    "왜 이렇게 고쳤냐" accountability + 노트 상태 섹션에 노출.
--  - ai_endpoints.disable_thinking: LLM thinking 비활성화 토글
--    (chat_template_kwargs.enable_thinking=false — llama.cpp/vLLM Qwen3 계열).
ALTER TABLE note_chat_messages ADD COLUMN parts TEXT;
ALTER TABLE note_bodies ADD COLUMN refine_request TEXT;
ALTER TABLE ai_endpoints ADD COLUMN disable_thinking INTEGER NOT NULL DEFAULT 0;
