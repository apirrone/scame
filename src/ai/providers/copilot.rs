use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use crate::ai::provider::{CompletionProvider, CompletionRequest, CompletionResponse};

/// GitHub Copilot completion provider
pub struct CopilotProvider {
    client: Client,
    api_token: String,
}

impl CopilotProvider {
    /// Create a new Copilot provider
    pub fn new(api_token: String) -> Self {
        Self {
            client: Client::new(),
            api_token,
        }
    }
}

#[async_trait]
impl CompletionProvider for CopilotProvider {
    async fn get_completion(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        // GitHub Copilot API endpoint (unofficial/reverse-engineered)
        // Note: This is a placeholder - actual Copilot API may differ
        let endpoint = "https://copilot-proxy.githubusercontent.com/v1/engines/copilot-codex/completions";

        let body = json!({
            "prompt": request.code_before_cursor,
            "suffix": request.code_after_cursor,
            "max_tokens": 100,
            "temperature": 0.0,
            "top_p": 1,
            "n": 1,
            "stop": ["\n\n"],
            "stream": false,
        });

        let response = self
            .client
            .post(endpoint)
            .bearer_auth(&self.api_token)
            .json(&body)
            .send()
            .await?;

        let data: serde_json::Value = response.json().await?;

        // Extract completion text from response
        let text = data["choices"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(CompletionResponse {
            text,
            provider: "copilot".to_string(),
        })
    }

    fn name(&self) -> &str {
        "copilot"
    }
}
