//! Process user input utilities - translates processUserInput.ts from TypeScript
//!
//! This module handles processing user input, including text prompts, bash commands,
//! slash commands, and attachments.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::Message;

/// Prompt input mode
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PromptInputMode {
    #[default]
    Prompt,
    Bash,
    Print,
    Continue,
}

/// Process user input context - combines ToolUseContext and LocalJSXCommandContext
/// (Extended to match TypeScript's rich context with memory/skill tracking)
#[derive(Debug, Clone)]
pub struct ProcessUserInputContext {
    /// Session ID
    pub session_id: String,
    /// Current working directory
    pub cwd: String,
    /// Agent ID if set
    pub agent_id: Option<String>,
    /// Query tracking information
    pub query_tracking: Option<QueryTracking>,
    /// Context options
    pub options: ProcessUserInputContextOptions,
    /// Track nested memory paths loaded via memory attachment triggers
    pub loaded_nested_memory_paths: std::collections::HashSet<String>,
    /// Track discovered skill names (feeds was_discovered on skill_tool_invocation)
    pub discovered_skill_names: std::collections::HashSet<String>,
    /// Trigger directories for dynamic skill loading
    pub dynamic_skill_dir_triggers: std::collections::HashSet<String>,
    /// Trigger paths for nested memory attachments
    pub nested_memory_attachment_triggers: std::collections::HashSet<String>,
}

/// Query tracking for analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryTracking {
    pub chain_id: String,
    pub depth: u32,
}

/// Process user input context options
#[derive(Debug, Clone)]
pub struct ProcessUserInputContextOptions {
    /// Available commands
    pub commands: Vec<Value>,
    /// Debug mode
    pub debug: bool,
    /// Available tools
    pub tools: Vec<crate::types::ToolDefinition>,
    /// Verbose mode
    pub verbose: bool,
    /// Main loop model
    pub main_loop_model: Option<String>,
    /// Thinking configuration
    pub thinking_config: Option<crate::types::api_types::ThinkingConfig>,
    /// MCP clients
    pub mcp_clients: Vec<Value>,
    /// MCP resources
    pub mcp_resources: std::collections::HashMap<String, Value>,
    /// IDE installation status
    pub ide_installation_status: Option<Value>,
    /// Non-interactive session flag
    pub is_non_interactive_session: bool,
    /// Custom system prompt
    pub custom_system_prompt: Option<String>,
    /// Append system prompt
    pub append_system_prompt: Option<String>,
    /// Agent definitions
    pub agent_definitions: AgentDefinitions,
    /// Theme
    pub theme: Option<String>,
    /// Max budget in USD
    pub max_budget_usd: Option<f64>,
}

impl Default for ProcessUserInputContext {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            cwd: String::new(),
            agent_id: None,
            query_tracking: None,
            options: ProcessUserInputContextOptions::default(),
            loaded_nested_memory_paths: std::collections::HashSet::new(),
            discovered_skill_names: std::collections::HashSet::new(),
            dynamic_skill_dir_triggers: std::collections::HashSet::new(),
            nested_memory_attachment_triggers: std::collections::HashSet::new(),
        }
    }
}

impl Default for ProcessUserInputContextOptions {
    fn default() -> Self {
        Self {
            commands: vec![],
            debug: false,
            tools: vec![],
            verbose: false,
            main_loop_model: None,
            thinking_config: None,
            mcp_clients: vec![],
            mcp_resources: std::collections::HashMap::new(),
            ide_installation_status: None,
            is_non_interactive_session: false,
            custom_system_prompt: None,
            append_system_prompt: None,
            agent_definitions: AgentDefinitions::default(),
            theme: None,
            max_budget_usd: None,
        }
    }
}

/// Agent definitions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefinitions {
    pub active_agents: Vec<Value>,
    pub all_agents: Vec<Value>,
    pub allowed_agent_types: Option<Vec<String>>,
}

/// Effort value for the model
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffortValue {
    pub effort: String,
    pub reason: Option<String>,
}

/// Result of processing user input
#[derive(Debug, Clone)]
pub struct ProcessUserInputBaseResult {
    /// Messages to be sent to the model
    pub messages: Vec<Message>,
    /// Whether a query should be made
    pub should_query: bool,
    /// Allowed tools (optional)
    pub allowed_tools: Option<Vec<String>>,
    /// Model to use (optional)
    pub model: Option<String>,
    /// Effort value (optional)
    pub effort: Option<EffortValue>,
    /// Output text for non-interactive mode
    pub result_text: Option<String>,
    /// Next input to prefilling (optional)
    pub next_input: Option<String>,
    /// Whether to submit next input
    pub submit_next_input: Option<bool>,
}

impl Default for ProcessUserInputBaseResult {
    fn default() -> Self {
        Self {
            messages: vec![],
            should_query: true,
            allowed_tools: None,
            model: None,
            effort: None,
            result_text: None,
            next_input: None,
            submit_next_input: None,
        }
    }
}

/// Input for process_user_input function
pub struct ProcessUserInputOptions {
    /// Input string or content blocks
    pub input: ProcessUserInput,
    /// Input before expansion (for ultraplan keyword detection)
    pub pre_expansion_input: Option<String>,
    /// Input mode
    pub mode: PromptInputMode,
    /// Context for processing
    pub context: ProcessUserInputContext,
    /// Pasted contents from the user
    pub pasted_contents: Option<std::collections::HashMap<u32, PastedContent>>,
    /// IDE selection
    pub ide_selection: Option<IdeSelection>,
    /// Existing messages
    pub messages: Option<Vec<Message>>,
    /// Function to set user input while processing
    pub set_user_input_on_processing: Option<Box<dyn Fn(Option<String>) + Send + Sync>>,
    /// UUID for the prompt
    pub uuid: Option<String>,
    /// Whether input is already being processed
    pub is_already_processing: Option<bool>,
    /// Query source
    pub query_source: Option<QuerySource>,
    /// Function to check if tool can be used
    pub can_use_tool: Option<crate::utils::hooks::CanUseToolFnJson>,
    /// Skip slash command processing
    pub skip_slash_commands: Option<bool>,
    /// Bridge origin (for remote control)
    pub bridge_origin: Option<bool>,
    /// Whether this is a meta message (system-generated)
    pub is_meta: Option<bool>,
    /// Skip attachment processing
    pub skip_attachments: Option<bool>,
}

impl Default for ProcessUserInputOptions {
    fn default() -> Self {
        Self {
            input: ProcessUserInput::String(String::new()),
            pre_expansion_input: None,
            mode: PromptInputMode::Prompt,
            context: ProcessUserInputContext::default(),
            pasted_contents: None,
            ide_selection: None,
            messages: None,
            set_user_input_on_processing: None,
            uuid: None,
            is_already_processing: None,
            query_source: None,
            can_use_tool: None,
            skip_slash_commands: None,
            bridge_origin: None,
            is_meta: None,
            skip_attachments: None,
        }
    }
}

/// User input - either string or content blocks
#[derive(Clone)]
pub enum ProcessUserInput {
    String(String),
    ContentBlocks(Vec<ContentBlockParam>),
}

impl std::fmt::Debug for ProcessUserInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessUserInput::String(s) => f.debug_tuple("String").field(s).finish(),
            ProcessUserInput::ContentBlocks(blocks) => {
                f.debug_tuple("ContentBlocks").field(blocks).finish()
            }
        }
    }
}

/// Content block parameter (similar to Anthropic SDK's ContentBlockParam)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentBlockParam {
    /// Text content block
    Text {
        /// Text content
        text: String,
    },
    /// Image content block
    Image {
        /// Image source
        source: ImageSource,
    },
    /// Tool use content block
    ToolUse {
        /// Tool use ID
        id: String,
        /// Tool name
        name: String,
        /// Tool input
        input: Value,
    },
    /// Tool result content block
    ToolResult {
        /// Tool use ID
        tool_use_id: String,
        /// Tool result content
        content: Value,
        /// Whether this is an error
        #[serde(default, skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

/// Image source for content blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageSource {
    /// Image type (base64)
    #[serde(rename = "type")]
    pub source_type: String,
    /// Media type (e.g., "image/png")
    pub media_type: String,
    /// Base64-encoded image data
    pub data: String,
}

/// Pasted content from user
#[derive(Debug, Clone)]
pub struct PastedContent {
    /// Unique ID
    pub id: u32,
    /// Content (base64-encoded)
    pub content: String,
    /// Media type
    pub media_type: Option<String>,
    /// Source path (optional)
    pub source_path: Option<String>,
    /// Dimensions (optional)
    pub dimensions: Option<ImageDimensions>,
}

/// Image dimensions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

/// IDE selection
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeSelection {
    /// File path
    pub file_path: String,
    /// Selected text
    pub selected_text: Option<String>,
    /// Cursor position
    pub cursor_position: Option<CursorPosition>,
}

/// Cursor position in IDE
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorPosition {
    pub line: u32,
    pub character: u32,
}

/// Query source enum
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuerySource {
    Prompt,
    Continue,
    SlashCommand,
    BashCommand,
    Attachments,
    AutoAttach,
    Resubmit,
}

/// Process user input - main entry point
///
/// # Arguments
/// * `options` - Options for processing user input
///
/// # Returns
/// A future that resolves to ProcessUserInputBaseResult
pub async fn process_user_input(
    options: ProcessUserInputOptions,
) -> Result<ProcessUserInputBaseResult, String> {
    let input_string = match &options.input {
        ProcessUserInput::String(s) => Some(s.clone()),
        ProcessUserInput::ContentBlocks(blocks) => blocks.iter().find_map(|b| {
            if let ContentBlockParam::Text { text } = b {
                Some(text.clone())
            } else {
                None
            }
        }),
    };

    // Set user input on processing if in prompt mode
    if options.mode == PromptInputMode::Prompt
        && input_string.is_some()
        && options.is_meta != Some(true)
    {
        if let Some(ref callback) = options.set_user_input_on_processing {
            callback(input_string.clone());
        }
    }

    // Process the input - take ownership of needed fields
    let input = options.input;
    let mode = options.mode;
    let context = options.context;
    let pasted_contents = options.pasted_contents;
    let uuid = options.uuid;
    let is_meta = options.is_meta;
    let skip_slash_commands = options.skip_slash_commands;
    let bridge_origin = options.bridge_origin;

    let result = process_user_input_base(
        input,
        mode,
        context,
        pasted_contents,
        uuid,
        is_meta,
        skip_slash_commands,
        bridge_origin,
    )
    .await?;

    // Execute user prompt submit hooks (simplified stub)
    // In the full implementation, this would execute hooks and potentially modify result

    Ok(result)
}

/// Internal function to process user input
async fn process_user_input_base(
    input: ProcessUserInput,
    mode: PromptInputMode,
    context: ProcessUserInputContext,
    pasted_contents: Option<std::collections::HashMap<u32, PastedContent>>,
    uuid: Option<String>,
    is_meta: Option<bool>,
    skip_slash_commands: Option<bool>,
    bridge_origin: Option<bool>,
) -> Result<ProcessUserInputBaseResult, String> {
    let input_string = match &input {
        ProcessUserInput::String(s) => Some(s.clone()),
        ProcessUserInput::ContentBlocks(blocks) => blocks.iter().find_map(|b| {
            if let ContentBlockParam::Text { text } = b {
                Some(text.clone())
            } else {
                None
            }
        }),
    };

    let mut preceding_input_blocks: Vec<ContentBlockParam> = vec![];
    let mut normalized_input = input.clone();

    // Handle content blocks - extract text and preceding blocks
    if let ProcessUserInput::ContentBlocks(blocks) = &input {
        if !blocks.is_empty() {
            let last_block = blocks.last().unwrap();
            if let ContentBlockParam::Text { text } = last_block {
                let text = text.clone();
                preceding_input_blocks = blocks[..blocks.len() - 1].to_vec();
                normalized_input = ProcessUserInput::String(text);
            } else {
                preceding_input_blocks = blocks.clone();
            }
        }
    }

    // Validate mode requires string input
    if input_string.is_none() && mode != PromptInputMode::Prompt {
        return Err(format!("Mode: {:?} requires a string input.", mode));
    }

    // Process pasted images
    let image_content_blocks = process_pasted_images(pasted_contents.as_ref()).await;

    // Check for bridge-safe slash command override
    let effective_skip_slash = check_bridge_safe_slash_command(
        bridge_origin,
        input_string.as_deref(),
        skip_slash_commands,
    );

    // Handle bash commands
    if let Some(input) = input_string {
        if mode == PromptInputMode::Bash {
            return process_bash_command(input, preceding_input_blocks, vec![], &context).await;
        }

        // Handle slash commands
        if !effective_skip_slash && input.starts_with('/') {
            return process_slash_command(
                input,
                preceding_input_blocks,
                image_content_blocks,
                vec![],
                &context,
            ).await;
        }
    }

    // Regular user prompt
    process_text_prompt(
        normalized_input,
        image_content_blocks,
        vec![],
        uuid,
        None, // permission_mode
        is_meta,
    )
}

/// Check if slash commands should be skipped for bridge origin
fn check_bridge_safe_slash_command(
    bridge_origin: Option<bool>,
    input_string: Option<&str>,
    skip_slash_commands: Option<bool>,
) -> bool {
    if bridge_origin != Some(true) {
        return skip_slash_commands.unwrap_or(false);
    }

    let input = match input_string {
        Some(s) => s,
        None => return skip_slash_commands.unwrap_or(false),
    };

    if !input.starts_with('/') {
        return skip_slash_commands.unwrap_or(false);
    }

    // For bridge origin with slash command, we don't skip
    false
}

/// Process pasted images
async fn process_pasted_images(
    pasted_contents: Option<&std::collections::HashMap<u32, PastedContent>>,
) -> Vec<ContentBlockParam> {
    if pasted_contents.is_none() {
        return vec![];
    }

    let contents = pasted_contents.unwrap();
    let mut image_blocks = vec![];

    for (_, pasted) in contents.iter() {
        let media_type = pasted.media_type.as_deref().unwrap_or("image/png");
        image_blocks.push(ContentBlockParam::Image {
            source: ImageSource {
                source_type: "base64".to_string(),
                media_type: media_type.to_string(),
                data: pasted.content.clone(),
            },
        });
    }

    image_blocks
}

/// Process text prompt
fn process_text_prompt(
    input: ProcessUserInput,
    _image_content_blocks: Vec<ContentBlockParam>,
    _attachment_messages: Vec<Message>,
    uuid: Option<String>,
    _permission_mode: Option<crate::types::api_types::PermissionMode>,
    is_meta: Option<bool>,
) -> Result<ProcessUserInputBaseResult, String> {
    let content = match input {
        ProcessUserInput::String(s) => {
            if s.trim().is_empty() {
                vec![]
            } else {
                vec![Value::String(s)]
            }
        }
        ProcessUserInput::ContentBlocks(blocks) => blocks
            .iter()
            .map(|b| serde_json::to_value(b).unwrap_or(Value::Null))
            .collect(),
    };

    let message = Message {
        role: crate::types::MessageRole::User,
        content: serde_json::json!({ "type": "text", "text": content }).to_string(),
        attachments: None,
        tool_call_id: None,
        tool_calls: None,
        is_api_error_message: None,
        error_details: None,
        is_error: None,
        is_meta: None,
            uuid: None,
    };

    Ok(ProcessUserInputBaseResult {
        messages: vec![message],
        should_query: true,
        ..Default::default()
    })
}

/// Format command input with XML tags
fn format_command_input_tags(command_name: &str, args: &str) -> String {
    let mut parts = vec![
        format!("<command-message>{}</command-message>", command_name),
        format!("<command-name>/{}</command-name>", command_name),
    ];
    if !args.trim().is_empty() {
        parts.push(format!("<command-args>{}</command-args>", args));
    }
    parts.join("\n")
}

/// Parsed slash command result
struct ParsedSlashCommand {
    command_name: String,
    args: String,
    is_mcp: bool,
}

/// Parses a slash command input string into its component parts.
/// Returns null if input doesn't start with '/' or is empty after stripping.
fn parse_slash_command(input: &str) -> Option<ParsedSlashCommand> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') || trimmed.len() <= 1 {
        return None;
    }
    let without_slash = &trimmed[1..];
    let words: Vec<&str> = without_slash.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }
    let mut command_name = words[0].to_string();
    let mut is_mcp = false;
    let mut args_start = 1;

    // Check for MCP commands (second word is '(MCP)')
    if words.len() > 1 && words[1] == "(MCP)" {
        command_name = format!("{} (MCP)", command_name);
        is_mcp = true;
        args_start = 2;
    }

    // Reconstruct args from original string to preserve spacing
    let args = if args_start < words.len() {
        // Find position after command name + (MCP) in the original string
        let skip_len = 1 + words[0].len(); // '/' + command
        let skip_len = if is_mcp {
            skip_len + 1 + 5 // space + "(MCP)"
        } else {
            skip_len + 1 // space
        };
        let skipped = trimmed.chars().skip(skip_len).collect::<String>();
        skipped.trim_start().to_string()
    } else {
        String::new()
    };

    Some(ParsedSlashCommand {
        command_name,
        args,
        is_mcp,
    })
}

/// Determines if a string looks like a valid command name.
/// Valid command names only contain letters, numbers, colons, hyphens, and underscores.
fn looks_like_command(command_name: &str) -> bool {
    command_name.chars().all(|c| {
        c.is_alphanumeric() || c == ':' || c == '-' || c == '_'
    })
}

/// Find a command by name or alias in the available commands.
fn find_command(name: &str, commands: &[serde_json::Value]) -> Option<serde_json::Value> {
    for cmd in commands {
        let cmd_name = cmd.get("name").and_then(|n| n.as_str()).unwrap_or("");
        if cmd_name == name {
            return Some(cmd.clone());
        }
        // Check aliases
        if let Some(aliases) = cmd.get("aliases").and_then(|a| a.as_array()) {
            for alias in aliases {
                if let Some(a) = alias.as_str() {
                    if a == name {
                        return Some(cmd.clone());
                    }
                }
            }
        }
    }
    None
}

/// Check if a command name exists in the available commands.
fn has_command(name: &str, commands: &[serde_json::Value]) -> bool {
    find_command(name, commands).is_some()
}

/// Create a user message with string content
fn make_user_message(content: String, is_meta: Option<bool>) -> Message {
    Message {
        role: crate::types::MessageRole::User,
        content,
        uuid: Some(uuid::Uuid::new_v4().to_string()),
        attachments: None,
        tool_call_id: None,
        tool_calls: None,
        is_error: None,
        is_meta,
        is_api_error_message: None,
        error_details: None,
    }
}

/// Create a system message
fn make_system_message(content: String) -> Message {
    Message {
        role: crate::types::MessageRole::System,
        content,
        uuid: Some(uuid::Uuid::new_v4().to_string()),
        attachments: None,
        tool_call_id: None,
        tool_calls: None,
        is_error: None,
        is_meta: None,
        is_api_error_message: None,
        error_details: None,
    }
}

/// Create a synthetic user caveat message (meta, invisible to user)
fn make_synthetic_caveat() -> Message {
    Message {
        role: crate::types::MessageRole::User,
        content: "The user didn't say anything. Continue working.".to_string(),
        uuid: Some(uuid::Uuid::new_v4().to_string()),
        attachments: None,
        tool_call_id: None,
        tool_calls: None,
        is_error: None,
        is_meta: Some(true),
        is_api_error_message: None,
        error_details: None,
    }
}

/// Create a system local command message
fn make_system_local_command(content: String) -> Message {
    make_system_message(content)
}

/// Process bash command — dispatches to BashTool or PowerShellTool
async fn process_bash_command(
    input: String,
    _preceding_input_blocks: Vec<ContentBlockParam>,
    attachment_messages: Vec<Message>,
    context: &ProcessUserInputContext,
) -> Result<ProcessUserInputBaseResult, String> {
    let user_message = make_user_message(
        format!("<bash-input>{}</bash-input>", input),
        None,
    );

    // Shell resolution: PowerShell on Windows when enabled, otherwise Bash
    let use_powershell = crate::tools::config_tools::is_powershell_tool_enabled();

    let tool_result = if use_powershell {
        let ps_tool = crate::tools::powershell::PowerShellTool::new();
        ps_tool
            .execute(
                serde_json::json!({"command": input}),
                &crate::types::ToolContext {
                    cwd: context.cwd.clone(),
                    abort_signal: Default::default(),
                },
            )
            .await
    } else {
        let bash_tool = crate::tools::bash::BashTool::new();
        bash_tool
            .execute(
                serde_json::json!({"command": input}),
                &crate::types::ToolContext {
                    cwd: context.cwd.clone(),
                    abort_signal: Default::default(),
                },
            )
            .await
    };

    let escape_xml = crate::utils::xml::escape_xml;

    match tool_result {
        Ok(result) => {
            let stdout = if result.content.is_empty() {
                "".to_string()
            } else {
                result.content.clone()
            };
            let stderr = if result.is_error == Some(true) {
                "Command completed with errors".to_string()
            } else {
                String::new()
            };
            let output_message = make_user_message(
                format!(
                    "<bash-stdout>{}</bash-stdout><bash-stderr>{}</bash-stderr>",
                    escape_xml(&stdout),
                    escape_xml(&stderr)
                ),
                None,
            );
            let mut messages = vec![
                make_synthetic_caveat(),
                user_message,
            ];
            messages.extend(attachment_messages);
            messages.push(output_message);
            Ok(ProcessUserInputBaseResult {
                messages,
                should_query: false,
                ..Default::default()
            })
        }
        Err(e) => {
            let error_message = make_user_message(
                format!(
                    "<bash-stderr>Command failed: {}</bash-stderr>",
                    escape_xml(&e.to_string())
                ),
                None,
            );
            let mut messages = vec![
                make_synthetic_caveat(),
                user_message,
            ];
            messages.extend(attachment_messages);
            messages.push(error_message);
            Ok(ProcessUserInputBaseResult {
                messages,
                should_query: false,
                ..Default::default()
            })
        }
    }
}

/// Process slash command — dispatches to registered commands
async fn process_slash_command(
    input: String,
    preceding_input_blocks: Vec<ContentBlockParam>,
    _image_content_blocks: Vec<ContentBlockParam>,
    attachment_messages: Vec<Message>,
    context: &ProcessUserInputContext,
) -> Result<ProcessUserInputBaseResult, String> {
    let parsed = parse_slash_command(&input);
    let parsed = match parsed {
        Some(p) => p,
        None => {
            let error_msg = "Commands are in the form `/command [args]`".to_string();
            return Ok(ProcessUserInputBaseResult {
                messages: vec![
                    make_synthetic_caveat(),
                ]
                .into_iter()
                .chain(attachment_messages.into_iter())
                .chain(std::iter::once(make_user_message(error_msg.clone(), None)))
                .collect(),
                should_query: false,
                result_text: Some(error_msg),
                ..Default::default()
            });
        }
    };

    let ParsedSlashCommand {
        command_name,
        args,
        is_mcp: _is_mcp,
    } = parsed;

    // Check if command exists
    if !has_command(&command_name, &context.options.commands) {
        // Check if it looks like a file path — if not, report as unknown skill
        let fs = std::path::Path::new(&command_name);
        let is_file_path = fs.exists();

        if looks_like_command(&command_name) && !is_file_path {
            let unknown_msg = format!("Unknown skill: {}", command_name);
            let mut messages = vec![
                make_synthetic_caveat(),
            ];
            messages.extend(attachment_messages);
            messages.push(make_user_message(unknown_msg.clone(), None));
            if !args.trim().is_empty() {
                messages.push(make_system_message(
                    format!("Args from unknown skill: {}", args)
                ));
            }
            return Ok(ProcessUserInputBaseResult {
                messages,
                should_query: false,
                result_text: Some(unknown_msg),
                ..Default::default()
            });
        }

        // Not a command name — treat as regular text prompt
        let content = if preceding_input_blocks.is_empty() {
            input
        } else {
            // Include preceding blocks context
            format!("[{} blocks] {}", preceding_input_blocks.len(), input)
        };
        return Ok(ProcessUserInputBaseResult {
            messages: vec![make_user_message(content, None)]
                .into_iter()
                .chain(attachment_messages)
                .collect(),
            should_query: true,
            ..Default::default()
        });
    }

    let command = find_command(&command_name, &context.options.commands)
        .ok_or_else(|| format!("Command '{}' not found", command_name))?;

    let command_type = command.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match command_type {
        "local" => execute_local_command(command_name, args, command, preceding_input_blocks, attachment_messages, context).await,
        "prompt" => execute_prompt_command(command_name, args, command, preceding_input_blocks, attachment_messages, context).await,
        "local-jsx" => {
            // Not supported in headless Rust SDK — return text result
            let msg = format!("Command '/{}' requires a UI and is not available in this environment.", command_name);
            Ok(ProcessUserInputBaseResult {
                messages: vec![
                    make_synthetic_caveat(),
                    make_user_message(msg.clone(), None),
                ]
                .into_iter()
                .chain(attachment_messages)
                .collect(),
                should_query: false,
                result_text: Some(msg),
                ..Default::default()
            })
        }
        _ => {
            let msg = format!("Unknown command type: {}", command_type);
            Err(msg)
        }
    }
}

/// Execute a local command by dispatching to registered handlers
async fn execute_local_command(
    command_name: String,
    args: String,
    _command: serde_json::Value,
    _preceding_input_blocks: Vec<ContentBlockParam>,
    attachment_messages: Vec<Message>,
    _context: &ProcessUserInputContext,
) -> Result<ProcessUserInputBaseResult, String> {
    let input_display = format_command_input_tags(&command_name, &args);
    let user_message = make_user_message(input_display, None);

    // Dispatch to built-in local command handler
    let result = dispatch_local_command(&command_name, &args).await;

    match result {
        Ok(call_result) => {
            use crate::commands::version::CommandCallResult;
            match call_result.result_type.as_str() {
                "text" => {
                    let output = if call_result.value.is_empty() {
                        make_system_local_command(
                            "(no output)".to_string()
                        )
                    } else {
                        make_system_local_command(
                            format!("<local-command-stdout>{}</local-command-stdout>", call_result.value)
                        )
                    };
                    let mut messages = vec![
                        make_synthetic_caveat(),
                        user_message,
                    ];
                    messages.extend(attachment_messages);
                    messages.push(output);
                    Ok(ProcessUserInputBaseResult {
                        messages,
                        should_query: false,
                        result_text: Some(call_result.value),
                        ..Default::default()
                    })
                }
                "compact" => {
                    // Compact result — the compaction was already performed
                    let mut messages = vec![
                        make_synthetic_caveat(),
                        user_message,
                    ];
                    messages.extend(attachment_messages);
                    messages.push(make_system_local_command(
                        format!("<local-command-stdout>Conversation compacted</local-command-stdout>")
                    ));
                    Ok(ProcessUserInputBaseResult {
                        messages,
                        should_query: false,
                        result_text: Some("Conversation compacted".to_string()),
                        ..Default::default()
                    })
                }
                "skip" => Ok(ProcessUserInputBaseResult {
                    messages: vec![],
                    should_query: false,
                    ..Default::default()
                }),
                _ => Err(format!("Unknown local command result type: {}", call_result.result_type)),
            }
        }
        Err(e) => {
            let mut messages = vec![
                make_synthetic_caveat(),
                user_message,
            ];
            messages.extend(attachment_messages);
            messages.push(make_system_local_command(
                format!("<local-command-stderr>{}</local-command-stderr>", e)
            ));
            Ok(ProcessUserInputBaseResult {
                messages,
                should_query: false,
                ..Default::default()
            })
        }
    }
}

/// Dispatch to a built-in local command handler.
/// Matches command name to handler function.
async fn dispatch_local_command(
    name: &str,
    args: &str,
) -> Result<crate::commands::version::CommandCallResult, String> {
    match name {
        "clear" => handle_clear_command(args),
        "cost" => handle_cost_command(args),
        "compact" => handle_compact_command(args),
        "version" => handle_version_command(args),
        "model" => handle_model_command(args),
        _ => {
            // For commands that don't have a Rust handler yet, return a text result
            Ok(crate::commands::version::CommandCallResult::text(
                format!("Command '/{}' is registered but not yet implemented in this environment.", name)
            ))
        }
    }
}

/// Handle /clear command
fn handle_clear_command(args: &str) -> Result<crate::commands::version::CommandCallResult, String> {
    let target = args.trim().split_whitespace().next().unwrap_or("conversation");
    match target {
        "cache" => Ok(crate::commands::version::CommandCallResult::text(
            "Cache cleared."
        )),
        "all" => Ok(crate::commands::version::CommandCallResult::text(
            "Conversation and cache cleared."
        )),
        _ => Ok(crate::commands::version::CommandCallResult::text("")),
    }
}

/// Handle /cost command
fn handle_cost_command(_args: &str) -> Result<crate::commands::version::CommandCallResult, String> {
    // Cost tracking is session-state dependent; without access to the session
    // state we can only report that cost tracking is available.
    Ok(crate::commands::version::CommandCallResult::text(
        "Cost tracking is available through the session's cost tracker."
    ))
}

/// Handle /compact command
fn handle_compact_command(_args: &str) -> Result<crate::commands::version::CommandCallResult, String> {
    // Compact requires session state manipulation which is handled by the query engine.
    // Return a text result indicating compaction was requested.
    Ok(crate::commands::version::CommandCallResult {
        result_type: "compact".to_string(),
        value: "Compact requested".to_string(),
    })
}

/// Handle /version command
fn handle_version_command(_args: &str) -> Result<crate::commands::version::CommandCallResult, String> {
    let version = env!("CARGO_PKG_VERSION");
    Ok(crate::commands::version::CommandCallResult::text(version))
}

/// Handle /model command
fn handle_model_command(_args: &str) -> Result<crate::commands::version::CommandCallResult, String> {
    Ok(crate::commands::version::CommandCallResult::text(
        "Model configuration is managed through session settings."
    ))
}

/// Execute a prompt command by expanding its prompt and sending to the model
async fn execute_prompt_command(
    command_name: String,
    args: String,
    command: serde_json::Value,
    preceding_input_blocks: Vec<ContentBlockParam>,
    attachment_messages: Vec<Message>,
    _context: &ProcessUserInputContext,
) -> Result<ProcessUserInputBaseResult, String> {
    let input_display = format_command_input_tags(&command_name, &args);
    let progress_message = command.get("progressMessage").and_then(|p| p.as_str()).unwrap_or("Loading");
    let model = command.get("model").and_then(|m| m.as_str()).map(String::from);
    let allowed_tools = command
        .get("allowedTools")
        .and_then(|t| t.as_array())
        .map(|t| t.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<String>>());
    let effort = command.get("effort").and_then(|e| e.as_str()).map(|e| {
        crate::utils::process_user_input::EffortValue {
            effort: e.to_string(),
            reason: None,
        }
    });

    // Build prompt content from the command's content field
    let content = command.get("content").and_then(|c| c.as_str()).unwrap_or("");
    let prompt_text = if !args.trim().is_empty() {
        format!("{}\n\nArguments: {}", content, args)
    } else {
        content.to_string()
    };

    // If there are preceding input blocks, include them
    let full_content = if preceding_input_blocks.is_empty() {
        prompt_text
    } else {
        format!("[{} preceding blocks]\n{}", preceding_input_blocks.len(), prompt_text)
    };

    let mut messages = vec![
        make_system_message(
            format!("[{}] {}", progress_message, command_name)
        ),
        make_user_message(input_display, None),
    ];
    messages.extend(attachment_messages);
    messages.push(make_user_message(full_content, None));

    Ok(ProcessUserInputBaseResult {
        messages,
        should_query: true,
        allowed_tools,
        model,
        effort,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_user_input_default() {
        let options = ProcessUserInputOptions::default();
        assert!(matches!(options.input, ProcessUserInput::String(s) if s.is_empty()));
        assert_eq!(options.mode, PromptInputMode::Prompt);
    }

    #[test]
    fn test_process_text_prompt() {
        let result = process_text_prompt(
            ProcessUserInput::String("Hello".to_string()),
            vec![],
            vec![],
            Some("test-uuid".to_string()),
            None,
            Some(true),
        )
        .unwrap();

        assert!(result.should_query);
        assert_eq!(result.messages.len(), 1);
    }

    #[test]
    fn test_parse_slash_command_basic() {
        let parsed = parse_slash_command("/compact").unwrap();
        assert_eq!(parsed.command_name, "compact");
        assert_eq!(parsed.args, "");
        assert!(!parsed.is_mcp);
    }

    #[test]
    fn test_parse_slash_command_with_args() {
        let parsed = parse_slash_command("/model opus").unwrap();
        assert_eq!(parsed.command_name, "model");
        assert_eq!(parsed.args, "opus");
        assert!(!parsed.is_mcp);
    }

    #[test]
    fn test_parse_slash_command_mcp() {
        let parsed = parse_slash_command("/my-tool (MCP) arg1 arg2").unwrap();
        assert_eq!(parsed.command_name, "my-tool (MCP)");
        assert_eq!(parsed.args, "arg1 arg2");
        assert!(parsed.is_mcp);
    }

    #[test]
    fn test_parse_slash_command_no_slash() {
        assert!(parse_slash_command("hello").is_none());
    }

    #[test]
    fn test_parse_slash_command_empty() {
        assert!(parse_slash_command("/").is_none());
    }

    #[test]
    fn test_parse_slash_command_spaces_only() {
        assert!(parse_slash_command("/ ").is_none());
    }

    #[test]
    fn test_looks_like_command_valid() {
        assert!(looks_like_command("compact"));
        assert!(looks_like_command("my-command"));
        assert!(looks_like_command("my_command"));
        assert!(looks_like_command("my:command"));
        assert!(looks_like_command("cmd123"));
    }

    #[test]
    fn test_looks_like_command_invalid() {
        assert!(!looks_like_command("/var/log"));
        assert!(!looks_like_command("file.txt"));
        assert!(!looks_like_command("path/to/file"));
    }

    #[test]
    fn test_has_command() {
        let commands = vec![
            serde_json::json!({"name": "clear", "type": "local"}),
            serde_json::json!({"name": "compact", "type": "local", "aliases": ["summarize"]}),
        ];
        assert!(has_command("clear", &commands));
        assert!(has_command("compact", &commands));
        assert!(has_command("summarize", &commands));
        assert!(!has_command("unknown", &commands));
    }

    #[test]
    fn test_find_command_by_name() {
        let commands = vec![
            serde_json::json!({"name": "clear", "type": "local"}),
        ];
        let cmd = find_command("clear", &commands).unwrap();
        assert_eq!(cmd["name"], "clear");
    }

    #[test]
    fn test_find_command_by_alias() {
        let commands = vec![
            serde_json::json!({"name": "compact", "aliases": ["summarize"]}),
        ];
        let cmd = find_command("summarize", &commands).unwrap();
        assert_eq!(cmd["name"], "compact");
    }

    #[test]
    fn test_format_command_input_tags() {
        let tags = format_command_input_tags("compact", "");
        assert!(tags.contains("<command-message>compact</command-message>"));
        assert!(tags.contains("<command-name>/compact</command-name>"));
        assert!(!tags.contains("<command-args>"));
    }

    #[test]
    fn test_format_command_input_tags_with_args() {
        let tags = format_command_input_tags("model", "opus");
        assert!(tags.contains("<command-message>model</command-message>"));
        assert!(tags.contains("<command-name>/model</command-name>"));
        assert!(tags.contains("<command-args>opus</command-args>"));
    }

    #[tokio::test]
    async fn test_dispatch_clear_command() {
        let result = dispatch_local_command("clear", "").await.unwrap();
        assert_eq!(result.result_type, "text");
        assert_eq!(result.value, "");
    }

    #[tokio::test]
    async fn test_dispatch_clear_cache_command() {
        let result = dispatch_local_command("clear", "cache").await.unwrap();
        assert_eq!(result.result_type, "text");
        assert!(result.value.contains("Cache cleared"));
    }

    #[tokio::test]
    async fn test_dispatch_version_command() {
        let result = dispatch_local_command("version", "").await.unwrap();
        assert_eq!(result.result_type, "text");
        assert!(!result.value.is_empty());
    }

    #[tokio::test]
    async fn test_dispatch_unknown_command() {
        let result = dispatch_local_command("unknown-cmd", "").await.unwrap();
        assert_eq!(result.result_type, "text");
        assert!(result.value.contains("not yet implemented"));
    }

    #[tokio::test]
    async fn test_dispatch_compact_command() {
        let result = dispatch_local_command("compact", "").await.unwrap();
        assert_eq!(result.result_type, "compact");
    }

    #[tokio::test]
    async fn test_process_slash_command_invalid() {
        let context = ProcessUserInputContext::default();
        let result = process_slash_command(
            "hello".to_string(),
            vec![],
            vec![],
            vec![],
            &context,
        ).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(!r.should_query);
        assert!(r.result_text.as_ref().unwrap().contains("Commands are in the form"));
    }

    #[tokio::test]
    async fn test_process_slash_command_unknown() {
        let context = ProcessUserInputContext::default();
        let result = process_slash_command(
            "/nonexistent-command".to_string(),
            vec![],
            vec![],
            vec![],
            &context,
        ).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(!r.should_query);
        assert!(r.result_text.as_ref().unwrap().contains("Unknown skill"));
    }

    #[tokio::test]
    async fn test_process_slash_command_known_local() {
        let mut commands = vec![];
        commands.push(serde_json::json!({"name": "clear", "type": "local"}));
        let context = ProcessUserInputContext {
            options: ProcessUserInputContextOptions {
                commands,
                ..Default::default()
            },
            ..Default::default()
        };
        let result = process_slash_command(
            "/clear".to_string(),
            vec![],
            vec![],
            vec![],
            &context,
        ).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(!r.should_query);
    }

    #[tokio::test]
    async fn test_process_bash_command_echo() {
        let context = ProcessUserInputContext::default();
        let result = process_bash_command(
            "echo hello".to_string(),
            vec![],
            vec![],
            &context,
        ).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(!r.should_query);
        assert!(!r.messages.is_empty());
    }
}
