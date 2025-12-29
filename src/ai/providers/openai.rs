use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use crate::ai::provider::{CompletionProvider, CompletionRequest, CompletionResponse};

/// OpenAI completion provider
pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    model: String,
}

impl OpenAiProvider {
    /// Create a new OpenAI provider
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
        }
    }
}

#[async_trait]
impl CompletionProvider for OpenAiProvider {
    async fn get_completion(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let endpoint = "https://api.openai.com/v1/chat/completions";

        // Build prompt for code completion
        let prompt = format!(
            "Complete the following {} code. Only provide the completion, no explanations:\n\n{}",
            request.language, request.code_before_cursor
        );

        let body = json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": "You are a code completion assistant. Provide only the next line or few lines of code to complete the given code. Do not include explanations or markdown formatting."
                },
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "max_tokens": 100,
            "temperature": 0.2,
            "stop": ["\n\n"],
        });

        let response = self
            .client
            .post(endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;

        let data: serde_json::Value = response.json().await?;

        // Extract completion text from response
        let text = data["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();

        Ok(CompletionResponse {
            text,
            provider: "openai".to_string(),
        })
    }

    fn name(&self) -> &str {
        "openai"
    }
}
