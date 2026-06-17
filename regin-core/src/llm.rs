use anyhow::{Context, Result, anyhow};
use futures_util::stream::Stream;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tracing::{debug, trace};

use crate::types::ChatMessage;

/// Client for the NanoGPT API (OpenAI-compatible).
#[derive(Debug, Clone)]
pub struct NanoGptClient {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub client: Client,
}

/// Request body for chat completions.
#[derive(Debug, Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

/// Response body for non-streaming chat completions.
#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Option<ChoiceMessage>,
    delta: Option<ChoiceDelta>,
}

#[derive(Debug, Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChoiceDelta {
    content: Option<String>,
}

impl NanoGptClient {
    /// Create a new NanoGPT client.
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            client: Client::new(),
        }
    }

    /// Non-streaming chat completion. Returns the full response content.
    pub async fn chat_completion(&self, messages: &[ChatMessage]) -> Result<String> {
        let url = format!("{}/chat/completions", self.base_url);
        debug!(model = %self.model, num_messages = messages.len(), "Sending chat completion request");

        let request_body = ChatCompletionRequest {
            model: &self.model,
            messages,
            stream: None,
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request_body)
            .send()
            .await
            .context("Failed to send chat completion request")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Chat completion request failed with status {}: {}",
                status,
                body
            ));
        }

        let resp: ChatCompletionResponse = response
            .json()
            .await
            .context("Failed to parse chat completion response")?;

        let content = resp
            .choices
            .first()
            .and_then(|c| c.message.as_ref())
            .and_then(|m| m.content.clone())
            .unwrap_or_default();

        debug!(response_len = content.len(), "Chat completion received");
        Ok(content)
    }

    /// Streaming chat completion. Returns a stream of content delta tokens.
    ///
    /// Parses Server-Sent Events manually: reads lines, looks for `data: {...}` lines,
    /// extracts `choices[0].delta.content` from each.
    pub async fn chat_completion_stream(
        &self,
        messages: &[ChatMessage],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let url = format!("{}/chat/completions", self.base_url);
        debug!(model = %self.model, num_messages = messages.len(), "Sending streaming chat completion request");

        let request_body = ChatCompletionRequest {
            model: &self.model,
            messages,
            stream: Some(true),
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request_body)
            .send()
            .await
            .context("Failed to send streaming chat completion request")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Streaming chat completion request failed with status {}: {}",
                status,
                body
            ));
        }

        let byte_stream = response.bytes_stream();

        let stream = futures_util::stream::unfold(
            (byte_stream, String::new()),
            |(mut byte_stream, mut buffer)| async move {
                loop {
                    // Try to extract a complete line from the buffer
                    if let Some(newline_pos) = buffer.find('\n') {
                        let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
                        buffer = buffer[newline_pos + 1..].to_string();

                        // Skip empty lines
                        if line.is_empty() {
                            continue;
                        }

                        // Check for SSE data lines
                        if let Some(data) = line.strip_prefix("data: ") {
                            let data = data.trim();

                            // [DONE] signals end of stream
                            if data == "[DONE]" {
                                trace!("Stream finished: [DONE]");
                                return None;
                            }

                            // Parse JSON and extract delta content
                            match serde_json::from_str::<ChatCompletionResponse>(data) {
                                Ok(resp) => {
                                    if let Some(content) = resp
                                        .choices
                                        .first()
                                        .and_then(|c| c.delta.as_ref())
                                        .and_then(|d| d.content.clone())
                                    {
                                        if !content.is_empty() {
                                            trace!(token_len = content.len(), "Stream token");
                                            return Some((Ok(content), (byte_stream, buffer)));
                                        }
                                    }
                                    // Choice had no delta content, continue
                                    continue;
                                }
                                Err(e) => {
                                    return Some((
                                        Err(anyhow!("Failed to parse SSE data: {}: {}", e, data)),
                                        (byte_stream, buffer),
                                    ));
                                }
                            }
                        }

                        // Skip non-data SSE lines (e.g., comments, event:, id:)
                        continue;
                    }

                    // Need more data from the byte stream
                    match byte_stream.next().await {
                        Some(Ok(bytes)) => {
                            match std::str::from_utf8(&bytes) {
                                Ok(s) => buffer.push_str(s),
                                Err(e) => {
                                    return Some((
                                        Err(anyhow!("Invalid UTF-8 in SSE stream: {}", e)),
                                        (byte_stream, buffer),
                                    ));
                                }
                            }
                        }
                        Some(Err(e)) => {
                            return Some((
                                Err(anyhow!("Stream read error: {}", e)),
                                (byte_stream, buffer),
                            ));
                        }
                        None => {
                            // Stream ended; process any remaining data in buffer
                            if !buffer.is_empty() {
                                let line = std::mem::take(&mut buffer);
                                let line = line.trim();
                                if let Some(data) = line.strip_prefix("data: ") {
                                    let data = data.trim();
                                    if data != "[DONE]" {
                                        if let Ok(resp) =
                                            serde_json::from_str::<ChatCompletionResponse>(data)
                                        {
                                            if let Some(content) = resp
                                                .choices
                                                .first()
                                                .and_then(|c| c.delta.as_ref())
                                                .and_then(|d| d.content.clone())
                                            {
                                                if !content.is_empty() {
                                                    return Some((
                                                        Ok(content),
                                                        (byte_stream, String::new()),
                                                    ));
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
