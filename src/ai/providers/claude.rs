use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use crate::ai::provider::{CompletionProvider, CompletionRequest, CompletionResponse};

/// Claude completion provider (Anthropic)
pub struct ClaudeProvider {
    client: Client,
    api_key: String,
    model: String,
}

impl ClaudeProvider {
    /// Create a new Claude provider
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
        }
    }
}

#[async_trait]
impl CompletionProvider for ClaudeProvider {
    async fn get_completion(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let endpoint = "https://api.anthropic.com/v1/messages";

        // Build prompt for code completion
        // Use a system prompt to establish the behavior, and cache it for speed
        let system_prompt = format!(
            "You are a code completion assistant. When given code, complete it naturally. \
            If the code starts a function definition, class, or code block, provide the full implementation. \
            If it's mid-line, complete the line and add logical next lines. \
            Output ONLY raw code - no markdown fences, no explanations, no comments about what you're doing. \
            Language: {}",
            request.language
        );

        let body = json!({
            "model": self.model,
            "max_tokens": 1024,
            "system": [
                {
                    "type": "text",
                    "text": system_prompt,
                    "cache_control": {"type": "ephemeral"}
                }
            ],
            "messages": [
                {
                    "role": "user",
                    "content": request.code_before_cursor
                }
            ],
        });

        let response = self
            .client
            .post(endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;

        let data: serde_json::Value = response.json().await?;

        // Extract completion text from response
        let mut text = data["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();

        // Strip markdown code fences if present
        // Remove opening fence (```python, ```rust, etc.)
        if text.starts_with("```") {
            if let Some(newline_pos) = text.find('\n') {
                text = text[newline_pos + 1..].to_string();
            }
        }
        // Remove closing fence
        if text.ends_with("```") {
            if let Some(last_fence) = text.rfind("```") {
                text = text[..last_fence].trim_end().to_string();
            }
        }

        Ok(CompletionResponse {
            text,
            provider: "claude".to_string(),
        })
    }

    fn name(&self) -> &str {
        "claude"
    }
}
