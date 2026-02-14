use crate::lsp::config::Language;
use crate::lsp::protocol::{
    BufferId, Diagnostic, DiagnosticSeverity,
    LspRequest, LspResponse, Position,
};
use anyhow::Result;

// Debug logging helper
macro_rules! lsp_debug {
    ($($arg:tt)*) => {
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/scame/lsp_debug.log")
        {
            use std::io::Write;
            let _ = writeln!(&mut file, $($arg)*);
        }
    };
}
use lsp_types::{
    notification::{DidChangeTextDocument, DidOpenTextDocument, DidSaveTextDocument, Notification},
    request::{Initialize, Request},
    DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, InitializeParams,
    TextDocumentContentChangeEvent, TextDocumentIdentifier,
    TextDocumentItem, Url, VersionedTextDocumentIdentifier,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc;

/// LSP client for a specific language
struct LspClient {
    _process: Child,
    stdin: ChildStdin,
    stdout_reader: Option<BufReader<ChildStdout>>,
    next_request_id: i64,
    language: Language,
    buffer_id: BufferId,
}

impl LspClient {
    /// Start a new language server process
    async fn start(
        language: Language,
        buffer_id: BufferId,
        response_tx: mpsc::UnboundedSender<LspResponse>,
    ) -> Result<Self> {
        let (cmd, args) = language.server_command();

        // For Python, detect venv and set VIRTUAL_ENV
        let mut env_vars = std::collections::HashMap::new();
        if language == Language::Python {
            if let Ok(cwd) = std::env::current_dir() {
                // Try common venv directory names
                for venv_name in &[".venv", "venv", "env"] {
                    let venv_path = cwd.join(venv_name);
                    if venv_path.exists() && venv_path.is_dir() {
                        // Set VIRTUAL_ENV so pyright knows which Python to use
                        if let Some(venv_str) = venv_path.to_str() {
                            env_vars.insert("VIRTUAL_ENV", venv_str.to_string());
                            lsp_debug!("[LSP DEBUG] Setting VIRTUAL_ENV={} for pyright", venv_str);
                        }
                        break;
                    }
                }
            }
        }

        // Try primary command first
        let mut cmd_builder = Command::new(cmd);
        cmd_builder
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        // Set environment variables
        for (key, value) in &env_vars {
            cmd_builder.env(key, value);
        }

        let mut process = cmd_builder.spawn();

        // If primary fails, try alternatives
        if process.is_err() {
            for (alt_cmd, alt_args) in language.alternative_commands() {
                let mut cmd_builder = Command::new(alt_cmd);
                cmd_builder
                    .args(&alt_args)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null());

                // Set environment variables for alternatives too
                for (key, value) in &env_vars {
                    cmd_builder.env(key, value);
                }

                process = cmd_builder.spawn();

                if process.is_ok() {
                    break;
                }
            }
        }

        let mut process = process
            .map_err(|e| anyhow::anyhow!("Failed to start language server {}: {}. Try installing: {}", cmd, e, Self::installation_hint(&language)))?;

        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdin"))?;
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdout"))?;

        let stdout_reader = BufReader::new(stdout);

        let mut client = Self {
            _process: process,
            stdin,
            stdout_reader: Some(stdout_reader),
            next_request_id: 1,
            language,
            buffer_id,
        };

        // Send initialize request and wait for response
        client.initialize().await?;

        // Now spawn task to read responses, taking ownership of stdout_reader
        if let Some(stdout_reader) = client.stdout_reader.take() {
            tokio::spawn(async move {
                Self::read_responses(stdout_reader, response_tx, buffer_id).await;
            });
        }

        Ok(client)
    }

    /// Read one LSP message from the reader
    async fn read_one_message(reader: &mut BufReader<ChildStdout>) -> Result<String> {
        let mut content_length = 0;

        loop {
            let mut header = String::new();
            reader.read_line(&mut header).await?;

            if header.starts_with("Content-Length:") {
                if let Some(len_str) = header.strip_prefix("Content-Length:") {
                    content_length = len_str.trim().parse()?;
                }
            } else if header == "\r\n" && content_length > 0 {
                // Read the message body
                let mut body = vec![0u8; content_length];
                reader.read_exact(&mut body).await?;
                return Ok(String::from_utf8(body)?);
            }
        }
    }

    /// Send initialize request to the language server
    async fn initialize(&mut self) -> Result<()> {
        let capabilities = lsp_types::ClientCapabilities {
            text_document: Some(lsp_types::TextDocumentClientCapabilities {
                definition: Some(lsp_types::GotoCapability {
                    dynamic_registration: Some(false),
                    link_support: Some(true),
                }),
                completion: Some(lsp_types::CompletionClientCapabilities {
                    dynamic_registration: Some(false),
                    completion_item: Some(lsp_types::CompletionItemCapability {
                        snippet_support: Some(false),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Set root_uri and workspace_folders to current working directory so LSP can detect venv and configs
        let cwd = std::env::current_dir().ok();
        let root_uri = cwd.as_ref().and_then(|path| Url::from_file_path(path).ok());

        // Also set workspace_folders (modern LSP approach)
        let workspace_folders = root_uri.as_ref().map(|uri| {
            vec![lsp_types::WorkspaceFolder {
                uri: uri.clone(),
                name: cwd
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("workspace")
                    .to_string(),
            }]
        });

        lsp_debug!("[LSP DEBUG] Initializing with root_uri: {:?}", root_uri);
        lsp_debug!("[LSP DEBUG] Initializing with workspace_folders: {:?}", workspace_folders);

        let params = InitializeParams {
            process_id: Some(std::process::id()),
            root_uri,
            workspace_folders,
            capabilities,
            ..Default::default()
        };

        self.send_request::<Initialize>(params).await?;

        // Wait for the initialize response (required by LSP spec)
        if let Some(ref mut reader) = self.stdout_reader {
            match Self::read_one_message(reader).await {
                Ok(response) => {
                    lsp_debug!("[LSP DEBUG] Received initialize response: {}", response);

                    // Verify it's a valid initialize response
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&response) {
                        if let Some(error) = value.get("error") {
                            return Err(anyhow::anyhow!("LSP initialization failed: {:?}", error));
                        }
                    }
                }
                Err(e) => {
                    lsp_debug!("[LSP DEBUG] Failed to read initialize response: {}", e);
                    return Err(anyhow::anyhow!("Failed to read initialize response: {}", e));
                }
            }
        }

        // Send initialized notification (required by LSP spec)
        let initialized_notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        });

        self.send_message(&initialized_notification.to_string()).await?;
        lsp_debug!("[LSP DEBUG] Sent initialized notification");

        Ok(())
    }

    /// Read responses from language server
    async fn read_responses(
        mut reader: BufReader<ChildStdout>,
        response_tx: mpsc::UnboundedSender<LspResponse>,
        buffer_id: BufferId,
    ) {
        let mut content_length = 0;

        loop {
            let mut header = String::new();
            match reader.read_line(&mut header).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    if header.starts_with("Content-Length:") {
                        if let Some(len_str) = header.strip_prefix("Content-Length:") {
                            content_length = len_str.trim().parse().unwrap_or(0);
                        }
                    } else if header == "\r\n" && content_length > 0 {
                        // Read the message body
                        let mut body = vec![0u8; content_length];
                        if reader.read_exact(&mut body).await.is_ok() {
                            if let Ok(text) = String::from_utf8(body) {
                                Self::handle_message(&text, &response_tx, buffer_id);
                            }
                        }
                        content_length = 0;
                    }
                }
                Err(_) => break,
            }
        }
    }

    /// Handle a message from the language server
    fn handle_message(
        message: &str,
        response_tx: &mpsc::UnboundedSender<LspResponse>,
        buffer_id: BufferId,
    ) {
        lsp_debug!("[LSP DEBUG] Received message: {}", message);

        if let Ok(value) = serde_json::from_str::<serde_json::Value>(message) {
            // Check if it's a notification (has "method" but no "id")
            if let Some(method) = value.get("method").and_then(|m| m.as_str()) {
                if method == "textDocument/publishDiagnostics" {
                    if let Some(params) = value.get("params") {
                        if let Ok(diag_params) =
                            serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(
                                params.clone(),
                            )
                        {
                            let diagnostics = Self::convert_diagnostics(diag_params.diagnostics);
                            let _ = response_tx.send(LspResponse::Diagnostics {
                                buffer_id,
                                diagnostics,
                            });
                        }
                    }
                }
            }
            // Check if it's a response (has "id" field)
            else if value.get("id").is_some() {
                lsp_debug!("[LSP DEBUG] This is a response message with id: {:?}", value.get("id"));

                // Check for error response
                if let Some(error) = value.get("error") {
                    lsp_debug!("[LSP DEBUG] Error response: {:?}", error);
                    if let Some(message) = error.get("message").and_then(|m| m.as_str()) {
                        let _ = response_tx.send(LspResponse::Error {
                            message: format!("LSP error: {}", message),
                        });
                    }
                    return;
                }

                // Handle responses (skip initialize response)
                // Initialize response has "capabilities" in result, not location data
                if let Some(result) = value.get("result") {
                    // Skip initialize/capabilities responses
                    if result.get("capabilities").is_some() {
                        lsp_debug!("[LSP DEBUG] Skipping initialize/capabilities response");
                        return;
                    }
                    lsp_debug!("[LSP DEBUG] Result field: {:?}", result);

                    // Check if result is null
                    if result.is_null() {
                        lsp_debug!("[LSP DEBUG] Result is null");
                        return;
                    }

                    // Try to parse as CompletionResponse first
                    // CompletionList has "items" field
                    if let Some(items_field) = result.get("items") {
                        lsp_debug!("[LSP DEBUG] Detected CompletionList response");
                        if let Ok(completion_list) = serde_json::from_value::<lsp_types::CompletionList>(result.clone()) {
                            lsp_debug!("[LSP DEBUG] Successfully parsed CompletionList with {} items", completion_list.items.len());
                            let items = Self::convert_completion_items(completion_list.items);
                            let _ = response_tx.send(LspResponse::Completion { items });
                            return;
                        }
                    }
                    // Try to parse as Vec<CompletionItem>
                    else if result.is_array() {
                        lsp_debug!("[LSP DEBUG] Result is array, trying to parse as completions");
                        if let Ok(completion_items) = serde_json::from_value::<Vec<lsp_types::CompletionItem>>(result.clone()) {
                            lsp_debug!("[LSP DEBUG] Successfully parsed as Vec<CompletionItem> with {} items", completion_items.len());
                            let items = Self::convert_completion_items(completion_items);
                            let _ = response_tx.send(LspResponse::Completion { items });
                            return;
                        }
                    }

                    // Try to parse as GotoDefinitionResponse
                    lsp_debug!("[LSP DEBUG] Attempting to parse result as Location");
                    if let Ok(location) = serde_json::from_value::<lsp_types::Location>(result.clone()) {
                        lsp_debug!("[LSP DEBUG] Successfully parsed as Location: {:?}", location);
                        // Single location
                        if let Ok(path) = location.uri.to_file_path() {
                            let _ = response_tx.send(LspResponse::GotoDefinition {
                                location: crate::lsp::Location {
                                    path,
                                    position: Position::new(
                                        location.range.start.line as usize,
                                        location.range.start.character as usize,
                                    ),
                                },
                            });
                        }
                    } else {
                        lsp_debug!("[LSP DEBUG] Not a single Location, trying Vec<Location>");
                        if let Ok(locations) = serde_json::from_value::<Vec<lsp_types::Location>>(result.clone()) {
                            lsp_debug!("[LSP DEBUG] Successfully parsed as Vec<Location> with {} items", locations.len());
                            // Multiple locations - take the first one
                            if let Some(location) = locations.first() {
                                if let Ok(path) = location.uri.to_file_path() {
                                    let _ = response_tx.send(LspResponse::GotoDefinition {
                                        location: crate::lsp::Location {
                                            path,
                                            position: Position::new(
                                                location.range.start.line as usize,
                                                location.range.start.character as usize,
                                            ),
                                        },
                                    });
                                }
                            }
                        } else {
                            lsp_debug!("[LSP DEBUG] Not Vec<Location>, trying Vec<LocationLink>");
                            if let Ok(links) = serde_json::from_value::<Vec<lsp_types::LocationLink>>(result.clone()) {
                                lsp_debug!("[LSP DEBUG] Successfully parsed as Vec<LocationLink> with {} items", links.len());
                                // LocationLink array (newer LSP spec)
                                if let Some(link) = links.first() {
                                    if let Ok(path) = link.target_uri.to_file_path() {
                                        let _ = response_tx.send(LspResponse::GotoDefinition {
                                            location: crate::lsp::Location {
                                                path,
                                                position: Position::new(
                                                    link.target_selection_range.start.line as usize,
                                                    link.target_selection_range.start.character as usize,
                                                ),
                                            },
                                        });
                                    }
                                }
                            } else {
                                lsp_debug!("[LSP DEBUG] Failed to parse result as any known type. Raw result: {:?}", result);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Convert LSP diagnostics to our internal format
    fn convert_diagnostics(lsp_diagnostics: Vec<lsp_types::Diagnostic>) -> Vec<Diagnostic> {
        lsp_diagnostics
            .into_iter()
            .map(|d| Diagnostic {
                range: (
                    Position::new(d.range.start.line as usize, d.range.start.character as usize),
                    Position::new(d.range.end.line as usize, d.range.end.character as usize),
                ),
                severity: match d.severity {
                    Some(lsp_types::DiagnosticSeverity::ERROR) => DiagnosticSeverity::Error,
                    Some(lsp_types::DiagnosticSeverity::WARNING) => DiagnosticSeverity::Warning,
                    Some(lsp_types::DiagnosticSeverity::INFORMATION) => {
                        DiagnosticSeverity::Information
                    }
                    Some(lsp_types::DiagnosticSeverity::HINT) => DiagnosticSeverity::Hint,
                    _ => DiagnosticSeverity::Information,
                },
                message: d.message,
            })
            .collect()
    }

    /// Convert LSP completion items to our internal format
    fn convert_completion_items(lsp_items: Vec<lsp_types::CompletionItem>) -> Vec<crate::lsp::CompletionItem> {
        lsp_items
            .into_iter()
            .map(|item| crate::lsp::CompletionItem {
                label: item.label.clone(),
                kind: item.kind.and_then(|k| match k {
                    lsp_types::CompletionItemKind::FUNCTION => Some(crate::lsp::CompletionItemKind::Function),
                    lsp_types::CompletionItemKind::METHOD => Some(crate::lsp::CompletionItemKind::Method),
                    lsp_types::CompletionItemKind::VARIABLE => Some(crate::lsp::CompletionItemKind::Variable),
                    lsp_types::CompletionItemKind::FIELD => Some(crate::lsp::CompletionItemKind::Field),
                    lsp_types::CompletionItemKind::KEYWORD => Some(crate::lsp::CompletionItemKind::Keyword),
                    lsp_types::CompletionItemKind::MODULE => Some(crate::lsp::CompletionItemKind::Module),
                    lsp_types::CompletionItemKind::STRUCT => Some(crate::lsp::CompletionItemKind::Struct),
                    lsp_types::CompletionItemKind::ENUM => Some(crate::lsp::CompletionItemKind::Enum),
                    lsp_types::CompletionItemKind::INTERFACE => Some(crate::lsp::CompletionItemKind::Interface),
                    lsp_types::CompletionItemKind::CONSTANT => Some(crate::lsp::CompletionItemKind::Constant),
                    _ => Some(crate::lsp::CompletionItemKind::Other),
                }),
                detail: item.detail.clone(),
                insert_text: item.insert_text.or_else(|| Some(item.label.clone())),
            })
            .collect()
    }

    /// Send a request to the language server
    async fn send_request<R: Request>(&mut self, params: R::Params) -> Result<()> {
        let id = self.next_request_id;
        self.next_request_id += 1;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": R::METHOD,
            "params": params,
        });

        self.send_message(&request.to_string()).await
    }

    /// Send a notification to the language server
    async fn send_notification<N: Notification>(&mut self, params: N::Params) -> Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": N::METHOD,
            "params": params,
        });

        self.send_message(&notification.to_string()).await
    }

    /// Send a message to the language server
    async fn send_message(&mut self, message: &str) -> Result<()> {
        let header = format!("Content-Length: {}\r\n\r\n", message.len());
        self.stdin.write_all(header.as_bytes()).await?;
        self.stdin.write_all(message.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Get installation hint for a language server
    fn installation_hint(language: &Language) -> &'static str {
        match language {
            Language::Rust => "rustup component add rust-analyzer",
            Language::Python => "pip install pyright",
        }
    }

    /// Handle didOpen notification
    async fn did_open(&mut self, path: PathBuf, content: String, language_id: String) -> Result<()> {
        // Convert to absolute path
        let abs_path = if path.is_absolute() {
            path.clone()
        } else {
            std::env::current_dir()?.join(&path)
        };

        lsp_debug!("[LSP DEBUG] didOpen for {:?} (absolute: {:?})", path, abs_path);

        let uri = Url::from_file_path(&abs_path)
            .map_err(|_| anyhow::anyhow!("Invalid file path: {:?}", abs_path))?;

        lsp_debug!("[LSP DEBUG] didOpen URI: {}", uri);

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri,
                language_id,
                version: 1,
                text: content,
            },
        };

        self.send_notification::<DidOpenTextDocument>(params).await
    }

    /// Handle didChange notification
    async fn did_change(&mut self, path: PathBuf, content: String, version: i32) -> Result<()> {
        // Convert to absolute path
        let abs_path = if path.is_absolute() {
            path.clone()
        } else {
            std::env::current_dir()?.join(&path)
        };

        let uri = Url::from_file_path(&abs_path)
            .map_err(|_| anyhow::anyhow!("Invalid file path: {:?}", abs_path))?;

        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri,
                version,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: content,
            }],
        };

        self.send_notification::<DidChangeTextDocument>(params).await
    }

    /// Handle didSave notification
    async fn did_save(&mut self, path: PathBuf) -> Result<()> {
        // Convert to absolute path
        let abs_path = if path.is_absolute() {
            path.clone()
        } else {
            std::env::current_dir()?.join(&path)
        };

        let uri = Url::from_file_path(&abs_path)
            .map_err(|_| anyhow::anyhow!("Invalid file path: {:?}", abs_path))?;

        let params = DidSaveTextDocumentParams {
            text_document: TextDocumentIdentifier { uri },
            text: None,
        };

        self.send_notification::<DidSaveTextDocument>(params).await
    }

    /// Handle goto definition request
    async fn goto_definition(&mut self, path: PathBuf, position: Position) -> Result<()> {
        // Convert to absolute path
        let abs_path = if path.is_absolute() {
            path.clone()
        } else {
            std::env::current_dir()?.join(&path)
        };

        lsp_debug!("[LSP DEBUG] Sending goto definition request for {:?} (absolute: {:?}) at line:{} col:{}", path, abs_path, position.line, position.column);

        let uri = Url::from_file_path(&abs_path)
            .map_err(|_| anyhow::anyhow!("Invalid file path: {:?}", abs_path))?;

        lsp_debug!("[LSP DEBUG] URI: {}", uri);

        let params = lsp_types::GotoDefinitionParams {
            text_document_position_params: lsp_types::TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: lsp_types::Position {
                    line: position.line as u32,
                    character: position.column as u32,
                },
            },
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        };

        self.send_request::<lsp_types::request::GotoDefinition>(params).await
    }

    /// Request completion suggestions at a given position
    async fn completion(&mut self, path: PathBuf, position: Position) -> Result<()> {
        // Convert to absolute path
        let abs_path = if path.is_absolute() {
            path.clone()
        } else {
            std::env::current_dir()?.join(&path)
        };

        lsp_debug!("[LSP DEBUG] Sending completion request for {:?} (absolute: {:?}) at line:{} col:{}", path, abs_path, position.line, position.column);

        let uri = Url::from_file_path(&abs_path)
            .map_err(|_| anyhow::anyhow!("Invalid file path: {:?}", abs_path))?;

        lsp_debug!("[LSP DEBUG] Completion URI: {}", uri);

        let params = lsp_types::CompletionParams {
            text_document_position: lsp_types::TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: lsp_types::Position {
                    line: position.line as u32,
                    character: position.column as u32,
                },
            },
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
            context: None,
        };

        self.send_request::<lsp_types::request::Completion>(params).await
    }
}

/// Main LSP task handler
pub async fn lsp_task_handler(
    mut request_rx: mpsc::UnboundedReceiver<LspRequest>,
    response_tx: mpsc::UnboundedSender<LspResponse>,
) {
    let mut clients: HashMap<String, LspClient> = HashMap::new();

    while let Some(request) = request_rx.recv().await {
        match request {
            LspRequest::DidOpen {
                buffer_id,
                path,
                content,
                language,
            } => {
                // Detect language from path
                if let Some(lang) = Language::from_path(&path) {
                    let key = lang.language_id().to_string();

                    // Create client if doesn't exist
                    if !clients.contains_key(&key) {
                        match LspClient::start(lang, buffer_id, response_tx.clone()).await {
                            Ok(client) => {
                                clients.insert(key.clone(), client);
                            }
                            Err(e) => {
                                let _ = response_tx.send(LspResponse::Error {
                                    message: format!("Failed to start LSP client: {}", e),
                                });
                                continue;
                            }
                        }
                    }

                    if let Some(client) = clients.get_mut(&key) {
                        let _ = client.did_open(path, content, language).await;
                    }
                }
            }
            LspRequest::DidChange {
                path,
                content,
                version,
                ..
            } => {
                if let Some(lang) = Language::from_path(&path) {
                    let key = lang.language_id().to_string();
                    if let Some(client) = clients.get_mut(&key) {
                        let _ = client.did_change(path, content, version).await;
                    }
                }
            }
            LspRequest::DidSave { path, .. } => {
                if let Some(lang) = Language::from_path(&path) {
                    let key = lang.language_id().to_string();
                    if let Some(client) = clients.get_mut(&key) {
                        let _ = client.did_save(path).await;
                    }
                }
            }
            LspRequest::GotoDefinition {
                path,
                position,
                ..
            } => {
                if let Some(lang) = Language::from_path(&path) {
                    let key = lang.language_id().to_string();
                    if let Some(client) = clients.get_mut(&key) {
                        let _ = client.goto_definition(path, position).await;
                    }
                }
            }
            LspRequest::Completion {
                buffer_id,
                path,
                position,
            } => {
                lsp_debug!("[TASK HANDLER DEBUG] Received completion request for buffer {} at {:?} line:{} col:{}", buffer_id, path, position.line, position.column);
                if let Some(lang) = Language::from_path(&path) {
                    let key = lang.language_id().to_string();
                    lsp_debug!("[TASK HANDLER DEBUG] Looking for client with key: {}", key);
                    lsp_debug!("[TASK HANDLER DEBUG] Available clients: {:?}", clients.keys().collect::<Vec<_>>());
                    if let Some(client) = clients.get_mut(&key) {
                        lsp_debug!("[TASK HANDLER DEBUG] Found client, calling completion...");
                        let result = client.completion(path, position).await;
                        lsp_debug!("[TASK HANDLER DEBUG] Completion call result: {:?}", result);
                    } else {
                        lsp_debug!("[TASK HANDLER DEBUG] No client found for key: {}", key);
                    }
                }
            }
            LspRequest::Shutdown => {
                break;
            }
        }
    }
}
