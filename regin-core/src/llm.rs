use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use futures_util::stream::Stream;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::pin::Pin;
use tracing::{debug, trace};

use crate::tools::{ToolCall, ToolDef};
use crate::types::ChatMessage;

/// The seam through which the daemon obtains LLM completions (FEAT-071 /
/// DISC-020). `MimirClient` is the production implementation (network);
/// [`FakeLlm`] is a canned-response test double. Object-safe (`dyn
/// LlmClient`) so `AppState` can hold one behind an `Arc` and callers in
/// other crates (`reflect`, `skills`, `regind`) can take `&dyn LlmClient`
/// without needing to know the concrete type.
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Single non-streaming completion with tool support.
    async fn chat_turn(&self, messages: &[Value], tools: Option<&[ToolDef]>) -> Result<LlmTurn>;

    /// Compute an embedding vector for `input`.
    async fn embedding(&self, input: &str, model: &str) -> Result<Vec<f32>>;

    /// Simple non-streaming completion (no tools). Default impl in terms of
    /// [`Self::chat_turn`]; `MimirClient` overrides it to reuse its inherent
    /// method directly (same behaviour, no double implementation).
    async fn chat_completion(&self, messages: &[ChatMessage]) -> Result<String> {
        let msgs: Vec<Value> = messages.iter().map(MimirClient::msg_to_value).collect();
        match self.chat_turn(&msgs, None).await? {
            LlmTurn::Text(t) => Ok(t),
            LlmTurn::ToolCalls { .. } => Err(anyhow!("Unexpected tool calls in simple completion")),
        }
    }
}

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

// -- Embedding response types --

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingDatum>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingDatum {
    embedding: Vec<f32>,
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

    /// Compute an embedding vector for `input` via `/v1/embeddings`.
    /// Returns an error when the API is unavailable or the model doesn't
    /// support embeddings (callers should log and degrade gracefully).
    pub async fn embedding(&self, input: &str, model: &str) -> Result<Vec<f32>> {
        let url = format!("{}/embeddings", self.base_url);
        let body = serde_json::json!({
            "model": model,
            "input": input,
        });

        let response = self
            .client
            .post(&url)
            .header(CERT_FINGERPRINT_HEADER, &self.fingerprint)
            .json(&body)
            .send()
            .await
            .context("Embedding request failed")?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow!("Embedding error {status}: {text}"));
        }

        let resp: EmbeddingResponse = response
            .json()
            .await
            .context("Failed to parse embedding response")?;

        resp.data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| anyhow!("No embedding data in response"))
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

#[async_trait]
impl LlmClient for MimirClient {
    async fn chat_turn(&self, messages: &[Value], tools: Option<&[ToolDef]>) -> Result<LlmTurn> {
        // Calls the inherent method above — inherent methods always take
        // priority over trait methods for a concrete receiver, so this is
        // not recursive.
        self.chat_turn(messages, tools).await
    }

    async fn chat_completion(&self, messages: &[ChatMessage]) -> Result<String> {
        self.chat_completion(messages).await
    }

    async fn embedding(&self, input: &str, model: &str) -> Result<Vec<f32>> {
        self.embedding(input, model).await
    }
}

/// A canned-response [`LlmClient`] for tests — no network (FEAT-071 /
/// DISC-020). Not `#[cfg(test)]`-gated: `regind`'s own test suite (a
/// different crate) needs it too.
#[derive(Default)]
pub struct FakeLlm {
    turns: std::sync::Mutex<std::collections::VecDeque<LlmTurn>>,
    completions: std::sync::Mutex<std::collections::VecDeque<String>>,
    embeddings: std::sync::Mutex<std::collections::VecDeque<Vec<f32>>>,
}

impl FakeLlm {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a reply for the next `chat_turn` call.
    pub fn push_turn(&self, turn: LlmTurn) -> &Self {
        self.turns.lock().unwrap().push_back(turn);
        self
    }

    /// Queue a reply for the next `chat_completion` call.
    pub fn push_completion(&self, text: impl Into<String>) -> &Self {
        self.completions.lock().unwrap().push_back(text.into());
        self
    }

    /// Queue a reply for the next `embedding` call.
    pub fn push_embedding(&self, v: Vec<f32>) -> &Self {
        self.embeddings.lock().unwrap().push_back(v);
        self
    }
}

#[async_trait]
impl LlmClient for FakeLlm {
    async fn chat_turn(&self, _messages: &[Value], _tools: Option<&[ToolDef]>) -> Result<LlmTurn> {
        self.turns.lock().unwrap().pop_front().ok_or_else(|| anyhow!("FakeLlm: no queued chat_turn reply"))
    }

    // Overridden (rather than relying on the default chat_turn-based impl) so
    // callers can queue plain text without also queuing a matching LlmTurn.
    async fn chat_completion(&self, _messages: &[ChatMessage]) -> Result<String> {
        self.completions.lock().unwrap().pop_front().ok_or_else(|| anyhow!("FakeLlm: no queued completion reply"))
    }

    async fn embedding(&self, _input: &str, _model: &str) -> Result<Vec<f32>> {
        self.embeddings.lock().unwrap().pop_front().ok_or_else(|| anyhow!("FakeLlm: no queued embedding reply"))
    }
}

#[cfg(test)]
mod llm_client_trait_tests {
    use super::*;

    #[tokio::test]
    async fn fake_llm_chat_turn_replays_queued_turns_in_order() {
        let fake = FakeLlm::new();
        fake.push_turn(LlmTurn::Text("first".into()));
        fake.push_turn(LlmTurn::Text("second".into()));
        let a = fake.chat_turn(&[], None).await.unwrap();
        let b = fake.chat_turn(&[], None).await.unwrap();
        assert!(matches!(a, LlmTurn::Text(t) if t == "first"));
        assert!(matches!(b, LlmTurn::Text(t) if t == "second"));
    }

    #[tokio::test]
    async fn fake_llm_chat_turn_errors_when_queue_exhausted() {
        let fake = FakeLlm::new();
        assert!(fake.chat_turn(&[], None).await.is_err());
    }

    #[tokio::test]
    async fn fake_llm_chat_completion_replays_plain_text() {
        let fake = FakeLlm::new();
        fake.push_completion("hello");
        assert_eq!(fake.chat_completion(&[]).await.unwrap(), "hello");
    }

    #[tokio::test]
    async fn fake_llm_embedding_replays_vector() {
        let fake = FakeLlm::new();
        fake.push_embedding(vec![0.1, 0.2, 0.3]);
        assert_eq!(fake.embedding("text", "model").await.unwrap(), vec![0.1, 0.2, 0.3]);
    }

    #[tokio::test]
    async fn dyn_llm_client_dispatches_through_the_trait_object() {
        let fake = FakeLlm::new();
        fake.push_completion("via trait object");
        let boxed: Box<dyn LlmClient> = Box::new(fake);
        assert_eq!(boxed.chat_completion(&[]).await.unwrap(), "via trait object");
    }
}
