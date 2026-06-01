-- Step 4: 어떤 채팅 메시지가 이 녹음을 첨부해 보냈는지 연결한다.
-- 유저 말풍선에 첨부 칩(🎤+분초, 재생)을 표시하는 데 쓰인다.
ALTER TABLE recordings ADD COLUMN chat_message_id TEXT;
