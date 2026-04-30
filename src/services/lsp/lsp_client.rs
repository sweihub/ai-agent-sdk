// Source: ~/claudecode/openclaudecode/src/services/lsp/LSPClient.ts
//! LSP Client - Language Server Protocol client
//!
//! Manages communication with an LSP server process via stdio using JSON-RPC.
//! Implements Content-Length framed message protocol for bidirectional communication.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use crate::services::lsp::types::LspStartOptions;
use crate::utils::debug::{log_for_debugging, DebugLogLevel};
use crate::utils::errors::error_message;
use crate::utils::subprocess_env::subprocess_env;

/// LSP error code for "content modified" - transient error during indexing
const LSP_ERROR_CONTENT_MODIFIED: i64 = -32801;

/// JSON-RPC request id type
type RequestId = u64;

/// Notification handler: synchronous closure
pub type NotificationHandler = Box<dyn Fn(serde_json::Value) + Send + Sync>;

/// Request handler: async closure
pub type RequestHandlerFn =
    Box<dyn Fn(serde_json::Value) -> Pin<Box<dyn Future<Output = serde_json::Value> + Send>> + Send + Sync>;

/// Pending request resolution channel
type PendingChannel = tokio::sync::oneshot::Sender<Result<serde_json::Value, String>>;

/// Internal mutable state for the LSP client (shared via Arc)
struct LspClientInternal {
    /// Server process handle - tokio mutex for async safety
    process: tokio::sync::Mutex<Option<tokio::process::Child>>,
    /// Server process stdin writer (for sending messages) - tokio mutex for async safety
    stdin: tokio::sync::Mutex<Option<tokio::process::ChildStdin>>,
    /// Server capabilities (set after initialize)
    capabilities: Mutex<Option<serde_json::Value>>,
    /// Flag: start failed
    start_failed: AtomicBool,
    /// Last start error message
    start_error: Mutex<Option<String>>,
}

/// LSP client - manages a single language server process and JSON-RPC communication
///
/// Created via [create_lsp_client]. The client manages the lifecycle of an LSP
/// server process: spawning it, sending the initialize handshake, and handling
/// bidirectional JSON-RPC communication over stdio.
///
/// State machine:
/// - Unstarted -> Starting (via start()) -> Initialized (via initialize())
/// - Initialized -> Stopping (via stop()) -> Unstarted
/// - Any state -> Error (on failure)
///
/// Clone creates a new handle to the same underlying process and state.
pub struct LspClient {
    /// Server name for logging
    server_name: String,
    /// Shared internal state
    internal: Arc<LspClientInternal>,
    /// Flag: whether initialization handshake completed
    is_initialized: Arc<AtomicBool>,
    /// Flag: currently stopping (to suppress error logs during shutdown)
    is_stopping: Arc<AtomicBool>,
    /// Flag: read loop has been spawned
    is_listening: Arc<AtomicBool>,
    /// Queue for pending notification handlers (registered before start)
    pending_notification_handlers: Arc<Mutex<Vec<(String, NotificationHandler)>>>,
    /// Queue for pending request handlers (registered before start)
    pending_request_handlers: Arc<Mutex<Vec<(String, Arc<RequestHandlerFn>)>>>,
    /// Active notification handlers: method -> list of handlers
    notification_handlers: Arc<Mutex<HashMap<String, Vec<NotificationHandler>>>>,
    /// Active request handlers: method -> Arc<handler>
    request_handlers: Arc<Mutex<HashMap<String, Arc<RequestHandlerFn>>>>,
    /// Pending requests: id -> response channel
    pending_requests: Arc<Mutex<HashMap<RequestId, PendingChannel>>>,
    /// Counter for generating unique request IDs
    request_id_counter: Arc<AtomicU64>,
    /// Crash callback: called when the server process exits unexpectedly
    on_crash: Arc<
        Mutex<Option<Box<dyn Fn(Box<dyn std::error::Error + Send + Sync>) + Send + Sync>>>,
    >,
}

impl Clone for LspClient {
    fn clone(&self) -> Self {
        Self {
            server_name: self.server_name.clone(),
            internal: self.internal.clone(),
            is_initialized: self.is_initialized.clone(),
            is_stopping: self.is_stopping.clone(),
            is_listening: self.is_listening.clone(),
            pending_notification_handlers: self.pending_notification_handlers.clone(),
            pending_request_handlers: self.pending_request_handlers.clone(),
            notification_handlers: self.notification_handlers.clone(),
            request_handlers: self.request_handlers.clone(),
            pending_requests: self.pending_requests.clone(),
            request_id_counter: self.request_id_counter.clone(),
            on_crash: self.on_crash.clone(),
        }
    }
}

impl LspClient {
    /// Get the server capabilities (set after initialize)
    pub fn capabilities(&self) -> Option<serde_json::Value> {
        self.internal.capabilities.lock().unwrap().clone()
    }

    /// Check whether the server has completed the initialize handshake
    pub fn is_initialized(&self) -> bool {
        self.is_initialized.load(Ordering::SeqCst)
    }

    /// Start the LSP server process and begin JSON-RPC communication.
    ///
    /// Spawns the server process with the given command and args, sets up
    /// stdio pipes for JSON-RPC communication, and starts the read loop.
    ///
    /// # Arguments
    /// * `command` - Path to the language server executable
    /// * `args` - Arguments to pass to the executable
    /// * `options` - Optional environment and working directory overrides
    pub async fn start(
        &self,
        command: String,
        args: Vec<String>,
        options: Option<LspStartOptions>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut env_map = subprocess_env();
        if let Some(opts) = &options {
            if let Some(env) = &opts.env {
                env_map.extend(env.clone());
            }
        }

        // Spawn the LSP server process
        let mut cmd = tokio::process::Command::new(&command);
        cmd.args(&args);
        cmd.envs(&env_map);
        if let Some(opts) = &options {
            if let Some(cwd) = &opts.cwd {
                cmd.current_dir(cwd);
            }
        }
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            format!("Failed to spawn LSP server '{}': {}", self.server_name, e)
        })?;

        let pid = child.id().unwrap_or(0);
        log_for_debugging(
            &format!("[LSP] Spawned server '{}' with PID {}", self.server_name, pid),
            DebugLogLevel::Debug,
        );

        // Extract stdio handles
        let mut stdin = child.stdin.take().ok_or_else(|| {
            format!("LSP server '{}' stdin not available", self.server_name)
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            format!("LSP server '{}' stdout not available", self.server_name)
        })?;
        let stderr = child.stderr.take();

        // Store the child process and stdin
        *self.internal.process.lock().await = Some(child);
        *self.internal.stdin.lock().await = Some(stdin);

        // Capture stderr for diagnostics
        if let Some(stderr_stream) = stderr {
            let server_name = self.server_name.clone();
            tokio::spawn(async move {
                let mut buf_reader = BufReader::new(stderr_stream);
                let mut buf = String::new();
                loop {
                    buf.clear();
                    match buf_reader.read_line(&mut buf).await {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            let output = buf.trim().to_string();
                            if !output.is_empty() {
                                log_for_debugging(
                                    &format!("[LSP SERVER {}] {}", server_name, output),
                                    DebugLogLevel::Debug,
                                );
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        // Apply pending notification handlers
        let applied_notif: Vec<(String, NotificationHandler)> = {
            let mut pending = self.pending_notification_handlers.lock().unwrap();
            pending.drain(..).collect()
        };
        for (method, handler) in applied_notif {
            log_for_debugging(
                &format!(
                    "[LSP] Applied queued notification handler for '{}.{}'",
                    self.server_name, method
                ),
                DebugLogLevel::Debug,
            );
            self.notification_handlers
                .lock()
                .unwrap()
                .entry(method)
                .or_insert_with(Vec::new)
                .push(handler);
        }

        // Apply pending request handlers
        let applied_req: Vec<(String, Arc<RequestHandlerFn>)> = {
            let mut pending = self.pending_request_handlers.lock().unwrap();
            pending.drain(..).collect()
        };
        for (method, handler) in applied_req {
            log_for_debugging(
                &format!(
                    "[LSP] Applied queued request handler for '{}.{}'",
                    self.server_name, method
                ),
                DebugLogLevel::Debug,
            );
            self.request_handlers.lock().unwrap().insert(method, handler);
        }

        // Spawn the read loop as a background task
        let internal = self.internal.clone();
        let notification_handlers = self.notification_handlers.clone();
        let request_handlers = self.request_handlers.clone();
        let pending_requests = self.pending_requests.clone();
        let is_stopping = self.is_stopping.clone();
        let on_crash = self.on_crash.clone();
        let server_name = self.server_name.clone();

        self.is_listening.store(true, Ordering::SeqCst);

        tokio::spawn(async move {
            if let Err(e) = run_read_loop(
                stdout,
                internal,
                notification_handlers,
                request_handlers,
                pending_requests,
                is_stopping.clone(),
                on_crash,
                server_name.clone(),
            )
            .await
            {
                if !is_stopping.load(Ordering::SeqCst) {
                    log_for_debugging(
                        &format!("[LSP] Read loop error for '{}': {}", server_name, e),
                        DebugLogLevel::Error,
                    );
                }
            }
        });

        log_for_debugging(
            &format!("[LSP] Client started for '{}'", self.server_name),
            DebugLogLevel::Debug,
        );
        Ok(())
    }

    /// Send the LSP initialize request and complete the handshake.
    ///
    /// Sends the `initialize` request with the given parameters (workspace info,
    /// client capabilities, etc.) and then sends the `initialized` notification.
    pub async fn initialize(
        &self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        self.check_start_failed()?;

        let result = self.send_request_raw("initialize".to_string(), params).await?;

        // Extract and store capabilities
        if let Some(caps) = result.get("capabilities") {
            *self.internal.capabilities.lock().unwrap() = Some(caps.clone());
        }

        // Send initialized notification (fire-and-forget)
        self.send_notification("initialized".to_string(), serde_json::json!({}))
            .await;

        self.is_initialized.store(true, Ordering::SeqCst);
        log_for_debugging(
            &format!("[LSP] Server '{}' initialized", self.server_name),
            DebugLogLevel::Debug,
        );

        Ok(result)
    }

    /// Send a JSON-RPC request and get back a raw serde_json::Value.
    ///
    /// This is the main entry point used by the server instance layer.
    pub async fn send_request_raw(
        &self,
        method: String,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        self.check_start_failed()?;

        if !self.is_initialized.load(Ordering::SeqCst) {
            return Err(format!("LSP server '{}' not initialized", self.server_name).into());
        }

        let id = self.request_id_counter.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending_requests.lock().unwrap().insert(id, tx);

        let message = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.write_message(&message).await?;

        match rx.await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(e)) => Err(e.into()),
            Err(e) => Err(format!("Request channel closed: {}", e).into()),
        }
    }

    /// Send a JSON-RPC notification (fire-and-forget, no response expected).
    pub async fn send_notification(
        &self,
        method: String,
        params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.check_start_failed()?;

        let message = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.write_message(&message).await?;

        Ok(())
    }

    /// Register a handler for incoming LSP notifications from the server.
    ///
    /// If called before `start()`, the handler is queued and applied once the
    /// connection is ready.
    pub fn on_notification(&self, method: String, handler: NotificationHandler) {
        if !self.is_listening.load(Ordering::SeqCst) {
            self.pending_notification_handlers
                .lock()
                .unwrap()
                .push((method.clone(), handler));
            log_for_debugging(
                &format!(
                    "[LSP] Queued notification handler for '{}.{}' (connection not ready)",
                    self.server_name, method
                ),
                DebugLogLevel::Debug,
            );
            return;
        }

        self.notification_handlers
            .lock()
            .unwrap()
            .entry(method)
            .or_insert_with(Vec::new)
            .push(handler);
    }

    /// Register a handler for incoming LSP requests from the server.
    ///
    /// Some LSP servers send requests TO the client (reverse direction),
    /// e.g., `workspace/configuration`. This allows registering handlers.
    pub fn on_request(&self, method: String, handler: RequestHandlerFn) {
        let handler = Arc::new(handler);
        if !self.is_listening.load(Ordering::SeqCst) {
            self.pending_request_handlers
                .lock()
                .unwrap()
                .push((method.clone(), handler));
            log_for_debugging(
                &format!(
                    "[LSP] Queued request handler for '{}.{}' (connection not ready)",
                    self.server_name, method
                ),
                DebugLogLevel::Debug,
            );
            return;
        }

        self.request_handlers
            .lock()
            .unwrap()
            .insert(method, handler);
    }

    /// Stop the LSP server gracefully.
    ///
    /// Sends `shutdown` request and `exit` notification, then kills the process
    /// if it hasn't exited on its own. Clears internal state.
    pub async fn stop(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.is_stopping.store(true, Ordering::SeqCst);
        let mut shutdown_error: Option<Box<dyn std::error::Error + Send + Sync>> = None;

        // Try graceful shutdown via LSP protocol
        let _ = self
            .send_request_raw("shutdown".to_string(), serde_json::json!({}))
            .await;
        let _ = self
            .send_notification("exit".to_string(), serde_json::json!({}))
            .await;

        // Wait a bit for graceful shutdown, then force kill
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Kill the process if still alive
        {
            let mut process = self.internal.process.lock().await;
            let should_kill = process.as_mut()
                .map(|c| c.try_wait().ok().flatten().is_none())
                .unwrap_or(false);
            if should_kill {
                if let Some(ref mut child) = *process {
                    let _ = child.kill().await;
                }
            }
            *process = None;
        }

        // Clear stdin
        *self.internal.stdin.lock().await = None;

        // Clear state
        self.is_initialized.store(false, Ordering::SeqCst);
        *self.internal.capabilities.lock().unwrap() = None;

        // Resolve all pending requests with an error
        for (_, tx) in self.pending_requests.lock().unwrap().drain() {
            let _ = tx.send(Err("Client stopped".to_string()));
        }

        self.is_stopping.store(false, Ordering::SeqCst);
        self.is_listening.store(false, Ordering::SeqCst);

        log_for_debugging(
            &format!("[LSP] Client stopped for '{}'", self.server_name),
            DebugLogLevel::Debug,
        );

        if let Some(err) = shutdown_error {
            Err(err)
        } else {
            Ok(())
        }
    }

    /// Write a JSON-RPC message to the server process stdin.
    async fn write_message(
        &self,
        message: &serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let body = serde_json::to_vec(message).map_err(|e| {
            format!("Failed to serialize JSON-RPC message: {}", error_message(&e as &dyn std::error::Error))
        })?;

        let header = format!("Content-Length: {}\r\n\r\n", body.len())
            .into_bytes();

        let mut stdin = self.internal.stdin.lock().await;
        let stdin = stdin.as_mut().ok_or_else(|| {
            format!("LSP server '{}' stdin not available", self.server_name)
        })?;

        stdin.write_all(&header).await.map_err(|e| {
            format!("Failed to write header to '{}': {}", self.server_name, e)
        })?;
        stdin.write_all(&body).await.map_err(|e| {
            format!("Failed to write body to '{}': {}", self.server_name, e)
        })?;
        stdin.flush().await.map_err(|e| {
            format!("Failed to flush '{}': {}", self.server_name, e)
        })?;

        Ok(())
    }

    /// Check if the start process failed and return an error if so.
    fn check_start_failed(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.internal.start_failed.load(Ordering::SeqCst) {
            let err_msg = self
                .internal
                .start_error
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| format!("LSP server '{}' failed to start", self.server_name));
            return Err(err_msg.into());
        }
        Ok(())
    }
}

/// Run the JSON-RPC read loop: read messages from stdout and dispatch them.
async fn run_read_loop(
    mut stdout: tokio::process::ChildStdout,
    internal: Arc<LspClientInternal>,
    notification_handlers: Arc<Mutex<HashMap<String, Vec<NotificationHandler>>>>,
    request_handlers: Arc<Mutex<HashMap<String, Arc<RequestHandlerFn>>>>,
    pending_requests: Arc<Mutex<HashMap<RequestId, PendingChannel>>>,
    is_stopping: Arc<AtomicBool>,
    on_crash: Arc<
        Mutex<Option<Box<dyn Fn(Box<dyn std::error::Error + Send + Sync>) + Send + Sync>>>,
    >,
    server_name: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Read Content-Length headers and message bodies
    let mut header_buf = Vec::new();
    let mut collecting_header = true;

    loop {
        if is_stopping.load(Ordering::SeqCst) {
            break;
        }

        let byte_result = {
            let mut buf = [0u8; 1];
            stdout.read_exact(&mut buf).await.map(|_| buf[0])
        };

        match byte_result {
            Ok(byte) => {
                if collecting_header {
                    header_buf.push(byte);

                    // Check for end of headers: \r\n\r\n
                    if header_buf.len() >= 4
                        && header_buf[header_buf.len() - 4] == b'\r'
                        && header_buf[header_buf.len() - 3] == b'\n'
                        && header_buf[header_buf.len() - 2] == b'\r'
                        && header_buf[header_buf.len() - 1] == b'\n'
                    {
                        collecting_header = false;

                        // Parse Content-Length
                        let header_str =
                            String::from_utf8_lossy(&header_buf[..header_buf.len() - 4]);
                        let content_length = parse_content_length(&header_str).map_err(|e| {
                            format!("Failed to parse Content-Length: {}", e)
                        })?;

                        // Read the message body
                        let mut body = vec![0u8; content_length];
                        stdout.read_exact(&mut body).await.map_err(|e| {
                            format!("Failed to read message body: {}", e)
                        })?;

                        // Parse and dispatch the message
                        dispatch_message_raw(
                            &body,
                            &notification_handlers,
                            &request_handlers,
                            &pending_requests,
                            &internal,
                            &on_crash,
                            &server_name,
                        )
                        .await;
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Process exited
                break;
            }
            Err(e) => {
                if !is_stopping.load(Ordering::SeqCst) {
                    internal.start_failed.store(true, Ordering::SeqCst);
                    *internal.start_error.lock().unwrap() = Some(e.to_string());
                    log_for_debugging(
                        &format!(
                            "[LSP] Read error for '{}': {}",
                            server_name,
                            error_message(&e as &dyn std::error::Error)
                        ),
                        DebugLogLevel::Error,
                    );
                }
                break;
            }
        }
    }

    // Process exited - check if it was unexpected
    if !is_stopping.load(Ordering::SeqCst) {
        let mut process = internal.process.lock().await;
        if let Some(child) = process.as_mut() {
            if let Some(status) = child.try_wait().ok().flatten() {
                if !status.success() {
                    let exit_code = status.code().unwrap_or(-1);
                    let crash_error = Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!(
                            "LSP server '{}' crashed with exit code {}",
                            server_name, exit_code
                        ),
                    ));
                    if let Some(callback) = on_crash.lock().unwrap().as_ref() {
                        callback(crash_error);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Parse Content-Length from the header string.
fn parse_content_length(header: &str) -> Result<usize, String> {
    for line in header.lines() {
        if let Some(len_str) = line.strip_prefix("Content-Length:") {
            return len_str
                .trim()
                .parse::<usize>()
                .map_err(|e| format!("Invalid Content-Length '{}': {}", len_str.trim(), e));
        }
    }
    Err(format!("No Content-Length header found in: {}", header))
}

/// Dispatch a raw message body to the appropriate handler.
async fn dispatch_message_raw(
    body: &[u8],
    notification_handlers: &Arc<Mutex<HashMap<String, Vec<NotificationHandler>>>>,
    request_handlers: &Arc<Mutex<HashMap<String, Arc<RequestHandlerFn>>>>,
    pending_requests: &Arc<Mutex<HashMap<RequestId, PendingChannel>>>,
    internal: &Arc<LspClientInternal>,
    _on_crash: &Mutex<
        Option<Box<dyn Fn(Box<dyn std::error::Error + Send + Sync>) + Send + Sync>>,
    >,
    _server_name: &str,
) {
    let message: serde_json::Value = match serde_json::from_slice(body) {
        Ok(msg) => msg,
        Err(e) => {
            log_for_debugging(
                &format!(
                    "[LSP] Failed to parse JSON: {} (body: {})",
                    e,
                    String::from_utf8_lossy(body)
                ),
                DebugLogLevel::Debug,
            );
            return;
        }
    };

    // Check if this is a response (has "id" and "result" or "error")
    if let Some(id_value) = message.get("id") {
        if let Some(id) = id_value.as_u64() {
            // Response with a result
            if let Some(result) = message.get("result") {
                if let Some(tx) = pending_requests.lock().unwrap().remove(&id) {
                    let _ = tx.send(Ok(result.clone()));
                }
                return;
            }
            // Response with an error
            if let Some(error) = message.get("error") {
                let error_msg = error
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown error")
                    .to_string();
                if let Some(tx) = pending_requests.lock().unwrap().remove(&id) {
                    let _ = tx.send(Err(error_msg));
                }
                return;
            }
        }
    }

    // Check if this is a request from the server (has "id" and "method")
    if let Some(id_value) = message.get("id") {
        if let Some(id) = id_value.as_u64() {
            if let Some(method) = message.get("method").and_then(|m| m.as_str()) {
                if let Some(params) = message.get("params").cloned() {
                    let handler = request_handlers.lock().unwrap().get(method).cloned();
                    if let Some(handler) = handler {
                        let response = handler(params).await;

                        // Send response back to the server
                        let response_message = if let Ok(resp) =
                            serde_json::to_value(&response)
                        {
                            serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": resp,
                            })
                        } else {
                            serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": response,
                            })
                        };

                        // Write response to stdin
                        if let Ok(body) = serde_json::to_vec(&response_message) {
                            let header =
                                format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
                            let mut stdin_guard = internal.stdin.lock().await;
                            if let Some(mut stdin) = stdin_guard.as_mut() {
                                let _ = stdin.write_all(&header).await;
                                let _ = stdin.write_all(&body).await;
                                let _ = stdin.flush().await;
                            }
                        }
                    }
                }
                return;
            }
        }
    }

    // This is a notification (no "id", has "method")
    if let Some(method) = message.get("method").and_then(|m| m.as_str()) {
        let method = method.to_string();
        let params =
            message.get("params").cloned().unwrap_or(serde_json::json!(null));

        {
            let guard = notification_handlers.lock().unwrap();
            if let Some(handlers) = guard.get(&method) {
                for handler in handlers {
                    handler(params.clone());
                }
            }
        }
    }
}

/// Create an LSP client instance.
///
/// # Arguments
/// * `server_name` - Name for logging and error messages
/// * `on_crash` - Optional callback invoked when the server process exits unexpectedly
pub fn create_lsp_client(
    server_name: &str,
    on_crash: Option<Box<dyn Fn(Box<dyn std::error::Error + Send + Sync>) + Send + Sync>>,
) -> LspClient {
    LspClient {
        server_name: server_name.to_string(),
        internal: Arc::new(LspClientInternal {
            process: tokio::sync::Mutex::new(None),
            stdin: tokio::sync::Mutex::new(None),
            capabilities: Mutex::new(None),
            start_failed: AtomicBool::new(false),
            start_error: Mutex::new(None),
        }),
        is_initialized: Arc::new(AtomicBool::new(false)),
        is_stopping: Arc::new(AtomicBool::new(false)),
        is_listening: Arc::new(AtomicBool::new(false)),
        pending_notification_handlers: Arc::new(Mutex::new(Vec::new())),
        pending_request_handlers: Arc::new(Mutex::new(Vec::new())),
        notification_handlers: Arc::new(Mutex::new(HashMap::new())),
        request_handlers: Arc::new(Mutex::new(HashMap::new())),
        pending_requests: Arc::new(Mutex::new(HashMap::new())),
        request_id_counter: Arc::new(AtomicU64::new(1)),
        on_crash: Arc::new(Mutex::new(on_crash)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_lsp_client() {
        let client = create_lsp_client("test-server", None);
        assert_eq!(client.server_name, "test-server");
        assert!(!client.is_initialized());
        assert!(client.capabilities().is_none());
    }

    #[test]
    fn test_lsp_error_content_modified() {
        assert_eq!(LSP_ERROR_CONTENT_MODIFIED, -32801);
    }

    #[test]
    fn test_parse_content_length() {
        assert_eq!(parse_content_length("Content-Length: 123\r\n").unwrap(), 123);
        assert_eq!(
            parse_content_length("Content-Length: 42\r\n\r\n").unwrap(),
            42
        );
    }

    #[test]
    fn test_parse_content_length_missing() {
        assert!(parse_content_length("Foo: Bar\r\n").is_err());
    }
}
