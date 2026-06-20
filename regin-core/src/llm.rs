use anyhow::{Context, Result, anyhow};
use futures_util::stream::Stream;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::pin::Pin;
use tracing::{debug, trace};

use crate::tools::{ToolCall, ToolDef};
use crate::types::ChatMessage;

/// Client for the **Mimir** gateway's OpenAI-compatible `/v1` surface.
///
/// Regin reaches its LLM only through Mimir (the on-premise gateway).
/// Mimir authenticates a consumer by the SHA-256 fingerprint of its
/// client cert, presented in the `X-Client-Cert-Sha256` header — the
/// agent's opaque access credential, provisioned and approved out of band
/// (e.g. by Dvalin via `PUT /api/mimir/v1/consumers/{fingerprint}`).
#[derive(Debug, Clone)]
pub struct MimirClient {
    pub base_url: String,
    /// The approved consumer credential (client-cert SHA-256 fingerprint),
    /// sent as `X-Client-Cert-Sha256`.
    pub fingerprint: String,
    pub model: String,
    pub client: Client,
}

/// Header Mimir reads to identify an approved consumer.
const CERT_FINGERPRINT_HEADER: &str = "X-Client-Cert-Sha256";

// -- Request types --

#[derive(Debug, Serialize)]
struct CompletionRequest {
    model: String,
    messages: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

// -- Response types --

#[derive(Debug, Deserialize)]
struct CompletionResponse {
    choices: Vec<RespChoice>,
}

#[derive(Debug, Deserialize)]
struct RespChoice {
    message: Option<RespMessage>,
    delta: Option<RespDelta>,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RespMessage {
    #[allow(dead_code)]
    role: Option<String>,
    content: Option<String>,
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Deserialize)]
struct RespDelta {
    content: Option<String>,
}

/// The outcome of a single LLM call with tools.
#[derive(Debug)]
pub enum LlmTurn {
    /// LLM produced text content — done.
    Text(String),
    /// LLM wants to call tools.
    ToolCalls {
        /// The raw assistant message (to feed back into the conversation)
        assistant_message: Value,
        /// The tool calls to execute
        calls: Vec<ToolCall>,
    },
}

impl MimirClient {
    pub fn new(
        base_url: impl Into<String>,
        fingerprint: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            fingerprint: fingerprint.into(),
            model: model.into(),
            client: Client::new(),
        }
    }

    /// Convert ChatMessage to JSON value for the API.
    pub fn msg_to_value(msg: &ChatMessage) -> Value {
        serde_json::json!({
            "role": msg.role,
            "content": msg.content
        })
    }

    /// Single non-streaming completion with tool support.
    /// Returns either text or tool calls.
    pub async fn chat_turn(
        &self,
        messages: &[Value],
        tools: Option<&[ToolDef]>,
    ) -> Result<LlmTurn> {
        let url = format!("{}/chat/completions", self.base_url);
        debug!(model = %self.model, n = messages.len(), "LLM turn");

        let body = CompletionRequest {
            model: self.model.clone(),
            messages: messages.to_vec(),
            tools: tools.map(|t| t.to_vec()),
            stream: None,
        };

        let response = self.client
            .post(&url)
            .header(CERT_FINGERPRINT_HEADER, &self.fingerprint)
            .json(&body)
            .send()
            .await
            .context("LLM request failed")?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow!("LLM error {status}: {text}"));
        }

        let raw: Value = response.json().await.context("Failed to parse LLM response")?;
        let resp: CompletionResponse = serde_json::from_value(raw.clone())
            .context("Failed to deserialize LLM response")?;

        let choice = resp.choices.into_iter().next()
            .ok_or_else(|| anyhow!("No choices in LLM response"))?;

        let msg = choice.message.ok_or_else(|| anyhow!("No message in choice"))?;

        // Check for tool calls
        if let Some(tool_calls) = msg.tool_calls {
            if !tool_calls.is_empty() {
                // Reconstruct the assistant message as raw JSON for the conversation
                let assistant_msg = raw["choices"][0]["message"].clone();
                return Ok(LlmTurn::ToolCalls {
                    assistant_message: assistant_msg,
                    calls: tool_calls,
                });
            }
        }

        Ok(LlmTurn::Text(msg.content.unwrap_or_default()))
    }

    /// Simple non-streaming completion (no tools).
    pub async fn chat_completion(&self, messages: &[ChatMessage]) -> Result<String> {
        let msgs: Vec<Value> = messages.iter().map(Self::msg_to_value).collect();
        match self.chat_turn(&msgs, None).await? {
            LlmTurn::Text(t) => Ok(t),
            LlmTurn::ToolCalls { .. } => Err(anyhow!("Unexpected tool calls in simple completion")),
        }
    }

    /// Build a tool result message for feeding back to the LLM.
    pub fn tool_result_message(tool_call_id: &str, content: &str) -> Value {
        serde_json::json!({
            "role": "tool",
            "tool_call_id": tool_call_id,
            "content": content
        })
    }

    /// Streaming chat completion (no tools). Returns token stream.
    pub async fn chat_completion_stream(
        &self,
        messages: &[ChatMessage],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let msgs: Vec<Value> = messages.iter().map(Self::msg_to_value).collect();
        self.stream_messages(&msgs).await
    }

    /// Streaming from raw message values (used by agentic loop for final response).
    pub async fn stream_messages(
        &self,
        messages: &[Value],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let url = format!("{}/chat/completions", self.base_url);
        debug!(model = %self.model, n = messages.len(), "Streaming request");

        let body = CompletionRequest {
            model: self.model.clone(),
            messages: messages.to_vec(),
            tools: None,
            stream: Some(true),
        };

        let response = self.client
            .post(&url)
            .header(CERT_FINGERPRINT_HEADER, &self.fingerprint)
            .json(&body)
            .send()
            .await
            .context("Streaming request failed")?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow!("LLM streaming error {status}: {text}"));
        }

        let byte_stream = response.bytes_stream();

        let stream = futures_util::stream::unfold(
            (byte_stream, String::new()),
            |(mut byte_stream, mut buffer)| async move {
                loop {
                    if let Some(newline_pos) = buffer.find('\n') {
                        let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
                        buffer = buffer[newline_pos + 1..].to_string();

                        if line.is_empty() { continue; }

                        if let Some(data) = line.strip_prefix("data: ") {
                            let data = data.trim();
                            if data == "[DONE]" {
                                trace!("Stream done");
                                return None;
                            }
                            match serde_json::from_str::<CompletionResponse>(data) {
                                Ok(resp) => {
                                    if let Some(content) = resp.choices.first()
                                        .and_then(|c| c.delta.as_ref())
                                        .and_then(|d| d.content.clone())
                                    {
                                        if !content.is_empty() {
                                            return Some((Ok(content), (byte_stream, buffer)));
                                        }
                                    }
                                    continue;
                                }
                                Err(e) => {
                                    return Some((
                                        Err(anyhow!("SSE parse error: {e}: {data}")),
                                        (byte_stream, buffer),
                                    ));
                                }
                            }
                        }
                        continue;
                    }

                    match byte_stream.next().await {
                        Some(Ok(bytes)) => match std::str::from_utf8(&bytes) {
                            Ok(s) => buffer.push_str(s),
                            Err(e) => return Some((Err(anyhow!("UTF-8 error: {e}")), (byte_stream, buffer))),
                        },
                        Some(Err(e)) => return Some((Err(anyhow!("Stream error: {e}")), (byte_stream, buffer))),
                        None => {
                            if !buffer.is_empty() {
                                let line = std::mem::take(&mut buffer);
                                if let Some(data) = line.trim().strip_prefix("data: ") {
                                    let data = data.trim();
                                    if data != "[DONE]" {
                                        if let Ok(resp) = serde_json::from_str::<CompletionResponse>(data) {
                                            if let Some(content) = resp.choices.first()
                                                .and_then(|c| c.delta.as_ref())
                                                .and_then(|d| d.content.clone())
                                            {
                                                if !content.is_empty() {
                                                    return Some((Ok(content), (byte_stream, String::new())));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            return None;
                        }
                    }
                }
            },
        );

        Ok(Box::pin(stream))
    }
}
