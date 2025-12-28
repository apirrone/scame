use crate::lsp::config::Language;
use crate::lsp::protocol::{BufferId, LspRequest, LspResponse, Position};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::mpsc;

/// Manages LSP communication between main thread and background task
pub struct LspManager {
    request_tx: mpsc::UnboundedSender<LspRequest>,
    document_versions: HashMap<PathBuf, i32>,
}

impl LspManager {
    /// Create a new LSP manager and spawn the background task
    pub fn new() -> (Self, mpsc::UnboundedReceiver<LspResponse>) {
        let (request_tx, request_rx) = mpsc::unbounded_channel();
        let (response_tx, response_rx) = mpsc::unbounded_channel();

        // Spawn background LSP task
        tokio::spawn(async move {
            crate::lsp::client::lsp_task_handler(request_rx, response_tx).await;
        });

        let manager = Self {
            request_tx,
            document_versions: HashMap::new(),
        };

        (manager, response_rx)
    }

    /// Send a request to the LSP background task
    fn send_request(&self, request: LspRequest) -> Result<()> {
        self.request_tx
            .send(request)
            .map_err(|e| anyhow::anyhow!("Failed to send LSP request: {}", e))
    }

    /// Notify LSP that a document was opened
    pub fn did_open(
        &mut self,
        buffer_id: BufferId,
        path: PathBuf,
        content: String,
        language: Language,
    ) -> Result<()> {
        self.document_versions.insert(path.clone(), 1);
        self.send_request(LspRequest::DidOpen {
            buffer_id,
            path,
            content,
            language: language.language_id().to_string(),
        })
    }

    /// Notify LSP that a document was changed
    pub fn did_change(
        &mut self,
        buffer_id: BufferId,
        path: PathBuf,
        content: String,
    ) -> Result<()> {
        // Get version first before sending request
        let version = {
            let v = self
                .document_versions
                .entry(path.clone())
                .and_modify(|v| *v += 1)
                .or_insert(1);
            *v
        };
        self.send_request(LspRequest::DidChange {
            buffer_id,
            path,
            content,
            version,
        })
    }

    /// Notify LSP that a document was saved
    pub fn did_save(&mut self, buffer_id: BufferId, path: PathBuf) -> Result<()> {
        self.send_request(LspRequest::DidSave { buffer_id, path })
    }

    /// Request to jump to definition at a position
    pub fn goto_definition(
        &mut self,
        buffer_id: BufferId,
        path: PathBuf,
        position: Position,
    ) -> Result<()> {
        self.send_request(LspRequest::GotoDefinition {
            buffer_id,
            path,
            position,
        })
    }

    /// Request completions at a position
    pub fn completion(
        &mut self,
        buffer_id: BufferId,
        path: PathBuf,
        position: Position,
    ) -> Result<()> {
        self.send_request(LspRequest::Completion {
            buffer_id,
            path,
            position,
        })
    }

    /// Shutdown the LSP client
    pub fn shutdown(&mut self) -> Result<()> {
        self.send_request(LspRequest::Shutdown)
    }
}
