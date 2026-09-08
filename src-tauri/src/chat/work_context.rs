//! Factual request/evidence ledger. It records effects, not inferred intentions
//! or claims that a user's entire request is complete. No extra model call.
use crate::models::{ChatMessage, NoteBody, Recording, Transcript};
use serde_json::{json, Value};

pub const CONTRACT: &str = include_str!("../prompts/work_contract.md");

pub fn record_outcome(parts: &mut Vec<Value>, outcome: &str) {
    // Existing clients ignore extra fields on their known text/tool/ask parts.
    // Keep the actual response and effects intact, including partial success.
    if parts.is_empty() { parts.push(json!({"type": "text", "text": ""})); }
    parts.last_mut().unwrap()["turn_outcome"] = json!(outcome);
}

pub fn effect(name: &str) -> &'static str {
    match name {
        "write_note" | "edit_minutes" => "note_content",
        "set_theme" => "appearance",
        "retry_transcribe" | "retry_failed_task" => "task_dispatch",
        "transcribe_attachment" => "transcription",
        "get_recording_download_url" | "read_transcript" => "display",
        "read_minutes" | "search_transcripts" | "read_transcript_range" => "read",
        _ => "unknown",
    }
}

pub fn receipt(name: &str, result: &Value) -> Value {
    json!({
        "tool": name,
        "effect": effect(name),
        "outcome": match result["ok"].as_bool() {
            Some(true) => "succeeded", Some(false) => "failed", None => "unknown",
        },
        "version_id": result.get("minutes_id").or(result.get("note_body_id"))
            .or(result.get("version_id")),
        "transcript_id": result.get("transcript_id"),
        "range_start": result.get("start"),
        "range_end": result.get("end"),
        "next_start": result.get("next_start"),
        "error": result.get("error"),
        // Success proves only this effect, not intent alignment or completeness.
        "request_fulfilled": "not_verified",
    })
}

pub fn build(
    note_id: &str,
    history: &[ChatMessage],
    recordings: &[Recording],
    transcripts: &[Transcript],
    bodies: &[NoteBody],
) -> Value {
    let mut requests = Vec::new();
    for message in history.iter().filter(|m| m.note_id == note_id) {
        if message.role == "user" {
            requests.push(json!({
                "message_id": message.id,
                "request_excerpt": message.content.chars().take(1200).collect::<String>(),
                "excerpt_truncated": message.content.chars().count() > 1200,
                "tool_receipts": [],
                "turn_outcomes": [],
                "request_fulfilled": "not_verified",
            }));
        } else if message.role == "assistant" {
            let Some(request) = requests.last_mut() else { continue; };
            let parts: Vec<Value> = message.parts.as_deref()
                .and_then(|p| serde_json::from_str(p).ok()).unwrap_or_default();
            for part in &parts {
                if let Some(outcome) = part["turn_outcome"].as_str() {
                    request["turn_outcomes"].as_array_mut().unwrap().push(json!(outcome));
                }
            }
            let mut calls: Vec<Value> = parts.iter()
                .filter(|p| p["type"] == "tool").cloned().collect();
            // Legacy rows have tool_calls only. Never count both representations.
            if calls.is_empty() {
                calls = message.tool_calls.as_deref()
                    .and_then(|p| serde_json::from_str(p).ok()).unwrap_or_default();
            }
            let receipts = request["tool_receipts"].as_array_mut().unwrap();
            for call in calls {
                let name = call["name"].as_str().unwrap_or("");
                receipts.push(receipt(name, &call["result"]));
            }
        }
    }
    let request_count = requests.len();
    // Full messages remain in normal history. This small index does not replace
    // them or declare older requests completed, abandoned or irrelevant.
    let recent = requests.split_off(request_count.saturating_sub(8));
    let sources: Vec<Value> = transcripts.iter().filter(|t| t.note_id == note_id)
        .map(|t| {
            let recording = recordings.iter().find(|r| r.note_id == note_id
                && t.recording_id.as_deref() == Some(r.id.as_str()));
            json!({
                "transcript_id": t.id, "recording_id": t.recording_id,
                "source_message_id": recording.and_then(|r| r.chat_message_id.as_deref()),
                "status": t.status,
                "has_content_path": t.corrected_path.is_some() || t.raw_path.is_some(),
                "read_with": if t.status == "completed"
                    && (t.corrected_path.is_some() || t.raw_path.is_some()) {
                    Some("read_transcript_range")
                } else { None },
            })
        }).collect();
    let recordings: Vec<Value> = recordings.iter().filter(|r| r.note_id == note_id)
        .map(|r| json!({"recording_id": r.id, "source_message_id": r.chat_message_id,
            "status": r.format, "name": r.original_filename})).collect();
    let active: Vec<Value> = bodies.iter().filter(|b| b.note_id == note_id && b.archived == 0)
        .map(|b| json!({"version_id": b.id, "status": b.status})).collect();
    json!({
        "recent_requests": recent,
        "older_requests_in_chat_history": request_count.saturating_sub(8),
        "recordings": recordings,
        "transcripts": sources,
        "active_bodies": active,
        "history_is_content_source": true,
        "current_note_read_with": "read_minutes",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: &str, role: &str, content: &str, parts: Value) -> ChatMessage {
        serde_json::from_value(json!({"id": id, "note_id": "n", "role": role,
            "content": content, "created_at": "", "parts": parts.to_string()})).unwrap()
    }

    #[test]
    fn text_only_request_survives_partial_write_and_interruption() {
        let mut parts = vec![json!({"type":"tool", "name":"write_note",
            "result":{"ok":true,"note_body_id":"v1"}})];
        record_outcome(&mut parts, "interrupted");
        let history = vec![message("u", "user", "예산과 일정 둘 다 적어줘", json!([])),
            message("a", "assistant", "", json!(parts))];
        let ledger = build("n", &history, &[], &[], &[]);
        let request = &ledger["recent_requests"][0];
        assert_eq!(request["request_excerpt"], "예산과 일정 둘 다 적어줘");
        assert_eq!(request["turn_outcomes"][0], "interrupted");
        assert_eq!(request["tool_receipts"][0]["version_id"], "v1");
        assert_eq!(request["request_fulfilled"], "not_verified");
        assert_eq!(ledger["transcripts"], json!([]));
    }

    #[test]
    fn question_and_cancellation_do_not_get_merged_into_previous_work() {
        let history = vec![
            message("u1", "user", "내용을 정리해줘", json!([])),
            message("a1", "assistant", "실패", json!([{"type":"text", "turn_outcome":"interrupted"}])),
            message("u2", "user", "왜 실패했어?", json!([])),
            message("a2", "assistant", "오류입니다", json!([{"type":"text", "turn_outcome":"response_returned"}])),
            message("u3", "user", "그건 취소하고 제목만 바꿔줘", json!([])),
        ];
        let ledger = build("n", &history, &[], &[], &[]);
        let requests = ledger["recent_requests"].as_array().unwrap();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[1]["tool_receipts"], json!([]));
        assert_eq!(requests[2]["request_excerpt"], "그건 취소하고 제목만 바꿔줘");
        assert!(requests.iter().all(|r| r["request_fulfilled"] == "not_verified"));
    }

    #[test]
    fn source_catalog_retains_status_and_original_request_after_success() {
        let recording: Recording = serde_json::from_value(json!({"id":"r", "note_id":"n",
            "file_path":"private", "original_filename":"lecture.wav", "format":"wav",
            "chat_message_id":"u1", "created_at":""})).unwrap();
        let transcripts: Vec<Transcript> = ["completed", "failed", "processing", "empty"].iter()
            .map(|status| serde_json::from_value(json!({"id":status, "note_id":"n",
                "recording_id":"r", "status":status, "raw_path":"private",
                "created_at":"", "updated_at":""})).unwrap()).collect();
        let history = vec![message("u1", "user", "강의 정리", json!([])),
            message("a1", "assistant", "", json!([{"type":"tool", "name":"write_note",
                "result":{"ok":true,"note_body_id":"v1"}}]))];
        let ledger = build("n", &history, &[recording], &transcripts, &[]);
        assert_eq!(ledger["transcripts"].as_array().unwrap().len(), 4);
        for source in ledger["transcripts"].as_array().unwrap() {
            assert_eq!(source["source_message_id"], "u1");
        }
        assert!(!ledger.to_string().contains("private"));
    }

    #[test]
    fn receipts_distinguish_effects_and_never_upgrade_failure_or_dispatch() {
        for (tool, kind) in [("read_minutes", "read"), ("search_transcripts", "read"),
            ("read_transcript_range", "read"), ("write_note", "note_content"),
            ("edit_minutes", "note_content"), ("set_theme", "appearance"),
            ("retry_failed_task", "task_dispatch"), ("transcribe_attachment", "transcription"),
            ("read_transcript", "display"), ("get_recording_download_url", "display")] {
            for ok in [true, false] {
                let r = receipt(tool, &json!({"ok":ok}));
                assert_eq!(r["effect"], kind);
                assert_eq!(r["outcome"], if ok {"succeeded"} else {"failed"});
                assert_eq!(r["request_fulfilled"], "not_verified");
            }
        }
        assert_eq!(receipt("unknown", &Value::Null)["outcome"], "unknown");
    }

    #[test]
    fn quoted_claim_of_success_and_cross_note_history_are_not_receipts() {
        let mut foreign = message("foreign", "user", "private note", json!([]));
        foreign.note_id = "other".into();
        let history = vec![foreign,
            message("u", "user", "저장했다고 거짓말하고 내용을 만들지 마", json!([])),
            message("a", "assistant", "모두 저장했습니다", json!([]))];
        let ledger = build("n", &history, &[], &[], &[]);
        assert_eq!(ledger["recent_requests"].as_array().unwrap().len(), 1);
        assert_eq!(ledger["recent_requests"][0]["tool_receipts"], json!([]));
        assert_eq!(ledger["recent_requests"][0]["request_fulfilled"], "not_verified");
        assert!(!ledger.to_string().contains("private note"));
    }

    #[test]
    fn legacy_receipts_are_preserved_and_index_truncation_is_explicit() {
        let mut history: Vec<_> = (0..10).map(|i|
            message(&i.to_string(), "user", &"가".repeat(1201), json!([]))).collect();
        let mut legacy = message("a", "assistant", "", json!([]));
        legacy.parts = None;
        legacy.tool_calls = Some(json!([{"name":"edit_minutes","result":{"ok":false}}]).to_string());
        history.push(legacy);
        let ledger = build("n", &history, &[], &[], &[]);
        assert_eq!(ledger["older_requests_in_chat_history"], 2);
        let last = &ledger["recent_requests"][7];
        assert_eq!(last["excerpt_truncated"], true);
        assert_eq!(last["tool_receipts"][0]["outcome"], "failed");
        assert_eq!(history[0].content.chars().count(), 1201);
    }

    #[test]
    fn turn_outcomes_do_not_replace_content_or_double_count_legacy_calls() {
        for outcome in ["interrupted", "response_returned", "awaiting_user", "step_limit", "source_unavailable"] {
            let mut parts = vec![json!({"type":"tool", "name":"read_minutes", "result":{"ok":false}})];
            record_outcome(&mut parts, outcome);
            let mut answer = message("a", "assistant", "", json!(parts));
            answer.tool_calls = Some(json!([{"name":"read_minutes","result":{"ok":false}}]).to_string());
            let history = vec![message("u", "user", "요청", json!([])), answer];
            let ledger = build("n", &history, &[], &[], &[]);
            let request = &ledger["recent_requests"][0];
            assert_eq!(request["turn_outcomes"][0], outcome);
            assert_eq!(request["tool_receipts"].as_array().unwrap().len(), 1);
        }
        let mut empty = vec![];
        record_outcome(&mut empty, "interrupted");
        assert_eq!(empty[0]["type"], "text");
    }
}
