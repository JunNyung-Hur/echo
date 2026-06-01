//! OpenAI-compatible LLM client for the worker tasks (Phase 2).
//!
//! Ports the old worker's `openai` usage: the client never auto-retries
//! (max_retries=0); we do task-level manual retry — 3 attempts with exponential
//! backoff (G-TASK-009). Markdown code fences that models wrap around HTML are
//! stripped (matches generate.py).

#![allow(dead_code)] // Phase 2: consumed by transcribe (normalizer) + generate.

use std::collections::BTreeMap;
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::json;

use crate::error::{Error, Result};
use crate::models::AiEndpoint;

/// Per-call ceiling, mirrors the old worker `LLM_TIMEOUT`.
const LLM_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug)]
pub struct ChatResult {
    pub content: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// One chat completion against an OpenAI-compatible endpoint.
///
/// invariant: G-TASK-009 — no client-side auto-retry; retry 3× here with
/// exponential backoff (2s, 4s), surfacing the last error if all fail.
pub async fn chat_completion(
    endpoint: &AiEndpoint,
    system_prompt: &str,
    user_content: &str,
) -> Result<ChatResult> {
    let url = format!(
        "{}/chat/completions",
        endpoint.api_base_url.trim_end_matches('/')
    );
    let api_key: &str = if endpoint.api_key.is_empty() {
        "dummy"
    } else {
        endpoint.api_key.as_str()
    };
    let payload = json!({
        "model": endpoint.model_id,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_content},
        ],
        "temperature": 0.2,
    });

    let client = reqwest::Client::builder()
        .timeout(LLM_TIMEOUT)
        .build()
        .map_err(|e| Error::Other(format!("http client build failed: {e}")))?;

    let mut last_err = String::from("llm call failed");
    for attempt in 0..3 {
        match client
            .post(&url)
            .bearer_auth(api_key)
            .json(&payload)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let v: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| Error::Other(format!("llm response parse failed: {e}")))?;
                let content = v["choices"][0]["message"]["content"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let input_tokens = v["usage"]["prompt_tokens"].as_i64().unwrap_or(0);
                let output_tokens = v["usage"]["completion_tokens"].as_i64().unwrap_or(0);
                return Ok(ChatResult {
                    content: strip_code_fences(&content),
                    input_tokens,
                    output_tokens,
                });
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let snippet: String = body.chars().take(300).collect();
                last_err = format!("llm http {status}: {snippet}");
            }
            Err(e) => last_err = format!("llm request error: {e}"),
        }
        if attempt < 2 {
            let secs = 2.0_f64 * 2.0_f64.powi(attempt);
            tokio::time::sleep(Duration::from_secs_f64(secs)).await;
        }
    }
    Err(Error::Other(last_err))
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Debug)]
pub struct ChatTurn {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
}

/// Chat completion with function-calling tools (non-streaming). Returns the
/// assistant content + any tool_calls. Same 3× retry as `chat_completion`
/// (G-TASK-009). `messages` is the full OpenAI message array (system + history
/// + user + prior tool results); `tools` is the gated spec list (may be empty).
pub async fn chat_with_tools(
    endpoint: &AiEndpoint,
    messages: &[serde_json::Value],
    tools: &[serde_json::Value],
) -> Result<ChatTurn> {
    let url = format!(
        "{}/chat/completions",
        endpoint.api_base_url.trim_end_matches('/')
    );
    let api_key: &str = if endpoint.api_key.is_empty() {
        "dummy"
    } else {
        endpoint.api_key.as_str()
    };
    let mut payload = json!({ "model": endpoint.model_id, "messages": messages, "temperature": 0.2 });
    if !tools.is_empty() {
        payload["tools"] = json!(tools);
    }

    let client = reqwest::Client::builder()
        .timeout(LLM_TIMEOUT)
        .build()
        .map_err(|e| Error::Other(format!("http client build failed: {e}")))?;

    let mut last_err = String::from("llm call failed");
    for attempt in 0..3 {
        match client
            .post(&url)
            .bearer_auth(api_key)
            .json(&payload)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let v: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| Error::Other(format!("llm response parse failed: {e}")))?;
                let msg = &v["choices"][0]["message"];
                let content = msg["content"].as_str().unwrap_or_default().to_string();
                let mut tool_calls = Vec::new();
                if let Some(tcs) = msg["tool_calls"].as_array() {
                    for tc in tcs {
                        let id = tc["id"].as_str().unwrap_or_default().to_string();
                        let name = tc["function"]["name"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                        let args = serde_json::from_str(args_str).unwrap_or_else(|_| json!({}));
                        tool_calls.push(ToolCall { id, name, args });
                    }
                }
                return Ok(ChatTurn {
                    content,
                    tool_calls,
                });
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let snippet: String = body.chars().take(300).collect();
                last_err = format!("llm http {status}: {snippet}");
            }
            Err(e) => last_err = format!("llm request error: {e}"),
        }
        if attempt < 2 {
            let secs = 2.0_f64 * 2.0_f64.powi(attempt);
            tokio::time::sleep(Duration::from_secs_f64(secs)).await;
        }
    }
    Err(Error::Other(last_err))
}

/// Streaming chat-with-tools. Forwards each assistant text delta to `on_delta`
/// as it arrives (for live UI streaming), accumulates tool_call deltas, and
/// returns the assembled turn. Single attempt (streaming retry is not
/// meaningful mid-stream); a connect/HTTP failure surfaces as Err.
pub async fn chat_with_tools_streaming(
    endpoint: &AiEndpoint,
    messages: &[serde_json::Value],
    tools: &[serde_json::Value],
    mut on_delta: impl FnMut(&str),
) -> Result<ChatTurn> {
    let url = format!(
        "{}/chat/completions",
        endpoint.api_base_url.trim_end_matches('/')
    );
    let api_key: &str = if endpoint.api_key.is_empty() {
        "dummy"
    } else {
        endpoint.api_key.as_str()
    };
    let mut payload = json!({ "model": endpoint.model_id, "messages": messages, "stream": true, "temperature": 0.2 });
    if !tools.is_empty() {
        payload["tools"] = json!(tools);
    }

    let client = reqwest::Client::builder()
        .timeout(LLM_TIMEOUT)
        .build()
        .map_err(|e| Error::Other(format!("http client build failed: {e}")))?;
    let resp = client
        .post(&url)
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await
        .map_err(|e| Error::Other(format!("llm request error: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(300).collect();
        return Err(Error::Other(format!("llm http {status}: {snippet}")));
    }

    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let mut content = String::new();
    // index → (id, name, arg_buf)
    let mut tool_accum: BTreeMap<u64, (String, String, String)> = BTreeMap::new();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| Error::Other(format!("llm stream error: {e}")))?;
        buf.push_str(&String::from_utf8_lossy(&bytes));
        loop {
            let Some(nl) = buf.find('\n') else { break };
            let line = buf[..nl].trim().to_string();
            buf.drain(..=nl);
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
                continue;
            };
            let delta = &v["choices"][0]["delta"];
            if let Some(c) = delta["content"].as_str() {
                if !c.is_empty() {
                    content.push_str(c);
                    on_delta(c);
                }
            }
            if let Some(tcs) = delta["tool_calls"].as_array() {
                for tc in tcs {
                    let idx = tc["index"].as_u64().unwrap_or(0);
                    let entry = tool_accum.entry(idx).or_default();
                    if let Some(id) = tc["id"].as_str() {
                        if !id.is_empty() {
                            entry.0 = id.to_string();
                        }
                    }
                    if let Some(n) = tc["function"]["name"].as_str() {
                        if !n.is_empty() {
                            entry.1 = n.to_string();
                        }
                    }
                    if let Some(a) = tc["function"]["arguments"].as_str() {
                        entry.2.push_str(a);
                    }
                }
            }
        }
    }

    let tool_calls: Vec<ToolCall> = tool_accum
        .into_values()
        .filter(|(_, name, _)| !name.is_empty())
        .map(|(id, name, args)| {
            let args_v = serde_json::from_str(if args.is_empty() { "{}" } else { &args })
                .unwrap_or_else(|_| json!({}));
            ToolCall {
                id: if id.is_empty() {
                    format!("call_{name}")
                } else {
                    id
                },
                name,
                args: args_v,
            }
        })
        .collect();

    Ok(ChatTurn {
        content,
        tool_calls,
    })
}

/// Strip a leading ```lang fence and trailing ``` that LLMs sometimes wrap
/// around HTML/text output (matches generate.py's regex strip).
pub fn strip_code_fences(s: &str) -> String {
    let t = s.trim();
    if !t.starts_with("```") {
        return t.to_string();
    }
    let after_open = match t.find('\n') {
        Some(nl) => &t[nl + 1..],
        None => return String::new(),
    };
    let trimmed = after_open.trim_end();
    trimmed
        .strip_suffix("```")
        .unwrap_or(trimmed)
        .trim_end()
        .to_string()
}
