use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use crate::ai::provider::{CompletionProvider, CompletionRequest, CompletionResponse};

/// Local LLM completion provider (e.g., Ollama)
pub struct LocalLlmProvider {
    client: Client,
    endpoint: String,
}

impl LocalLlmProvider {
    /// Create a new Local LLM provider
    pub fn new(endpoint: String) -> Self {
        Self {
            client: Client::new(),
            endpoint,
        }
    }
}

#[async_trait]
impl CompletionProvider for LocalLlmProvider {
    async fn get_completion(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        // Build prompt for code completion
        let prompt = format!(
            "Complete the following {} code. Only provide the next line or few lines:\n\n{}",
            request.language, request.code_before_cursor
        );

        // Ollama API format
        let body = json!({
            "model": "codellama",  // Default model, can be made configurable
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": 0.2,
                "num_predict": 100,
            }
        });

        let response = self
            .client
            .post(&self.endpoint)
            .json(&body)
            .send()
            .await?;

        let data: serde_json::Value = response.json().await?;

        // Extract completion text from response
        // Ollama returns "response" field
        let text = data["response"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();

        Ok(CompletionResponse {
            text,
            provider: "local".to_string(),
        })
    }

    fn name(&self) -> &str {
        "local"
    }
}
