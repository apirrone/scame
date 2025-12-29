use anyhow::Result;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::ai::provider::{CompletionProvider, CompletionRequest, CompletionResponse};
use crate::workspace::BufferId;

/// AI completion request
#[derive(Debug)]
pub enum AiRequest {
    /// Request a completion
    GetCompletion {
        request: CompletionRequest,
        buffer_id: BufferId,
    },
    /// Cancel any pending requests
    CancelPending,
    /// Shutdown the AI manager
    Shutdown,
}

/// AI completion response
#[derive(Debug, Clone)]
pub enum AiResponse {
    /// Completion result
    Completion {
        buffer_id: BufferId,
        text: String,
        provider: String,
    },
    /// Error occurred
    Error(String),
}

/// AI completion manager
pub struct AiManager {
    /// Channel to send requests to the background task
    request_tx: mpsc::UnboundedSender<AiRequest>,
}

impl AiManager {
    /// Create a new AI manager with the given provider
    pub fn new(provider: Arc<dyn CompletionProvider>) -> (Self, mpsc::UnboundedReceiver<AiResponse>) {
        let (request_tx, request_rx) = mpsc::unbounded_channel();
        let (response_tx, response_rx) = mpsc::unbounded_channel();

        // Spawn background task
        tokio::spawn(async move {
            Self::run_background_task(provider, request_rx, response_tx).await;
        });

        (Self { request_tx }, response_rx)
    }

    /// Send a completion request
    pub fn request_completion(&self, request: CompletionRequest, buffer_id: BufferId) -> Result<()> {
        self.request_tx
            .send(AiRequest::GetCompletion { request, buffer_id })
            .map_err(|e| anyhow::anyhow!("Failed to send AI request: {}", e))
    }

    /// Cancel any pending requests
    pub fn cancel_pending(&self) -> Result<()> {
        self.request_tx
            .send(AiRequest::CancelPending)
            .map_err(|e| anyhow::anyhow!("Failed to send cancel request: {}", e))
    }

    /// Shutdown the AI manager
    pub fn shutdown(&self) -> Result<()> {
        self.request_tx
            .send(AiRequest::Shutdown)
            .map_err(|e| anyhow::anyhow!("Failed to send shutdown request: {}", e))
    }

    /// Background task that processes AI requests
    async fn run_background_task(
        provider: Arc<dyn CompletionProvider>,
        mut request_rx: mpsc::UnboundedReceiver<AiRequest>,
        response_tx: mpsc::UnboundedSender<AiResponse>,
    ) {
        // Track the current pending request
        let mut pending_task: Option<tokio::task::JoinHandle<()>> = None;

        while let Some(request) = request_rx.recv().await {
            match request {
                AiRequest::GetCompletion { request, buffer_id } => {
                    // Cancel any pending task
                    if let Some(task) = pending_task.take() {
                        task.abort();
                    }

                    // Spawn new completion task
                    let provider_clone = provider.clone();
                    let response_tx_clone = response_tx.clone();

                    let task = tokio::spawn(async move {
                        match provider_clone.get_completion(request).await {
                            Ok(response) => {
                                let _ = response_tx_clone.send(AiResponse::Completion {
                                    buffer_id,
                                    text: response.text,
                                    provider: response.provider,
                                });
                            }
                            Err(e) => {
                                let _ = response_tx_clone.send(AiResponse::Error(e.to_string()));
                            }
                        }
                    });

                    pending_task = Some(task);
                }
                AiRequest::CancelPending => {
                    // Cancel the pending task
                    if let Some(task) = pending_task.take() {
                        task.abort();
                    }
                }
                AiRequest::Shutdown => {
                    // Cancel pending task and exit
                    if let Some(task) = pending_task.take() {
                        task.abort();
                    }
                    break;
                }
            }
        }
    }
}
