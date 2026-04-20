// Source: ~/claudecode/openclaudecode/src/types/message.ts
// Also includes: ~/claudecode/openclaudecode/src/utils/messages.ts (createAwaySummaryMessage)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Origin of a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageOrigin {
    #[serde(rename = "kind", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Base message type with common fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct MessageBase {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "parentUuid")]
    pub parent_uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "isMeta")]
    pub is_meta: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "isVirtual")]
    pub is_virtual: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "isCompactSummary")]
    pub is_compact_summary: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "toolUseResult")]
    pub tool_use_result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<MessageOrigin>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Attachment message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentMessage {
    #[serde(flatten)]
    pub base: MessageBase,
    #[serde(rename = "type")]
    pub message_type: String, // "attachment"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// User message with content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    #[serde(flatten)]
    pub base: MessageBase,
    #[serde(rename = "type")]
    pub message_type: String, // "user"
    pub message: UserMessageContent,
}

/// Content of a user message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessageContent {
    pub content: UserContent,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Content can be a string or an array of content blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UserContent {
    Text(String),
    Blocks(Vec<UserContentBlock>),
}

/// A content block within user message content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Assistant message with optional content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    #[serde(flatten)]
    pub base: MessageBase,
    #[serde(rename = "type")]
    pub message_type: String, // "assistant"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<AssistantMessageContent>,
}

/// Content of an assistant message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessageContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Progress message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressMessage {
    #[serde(flatten)]
    pub base: MessageBase,
    #[serde(rename = "type")]
    pub message_type: String, // "progress"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<serde_json::Value>,
}

/// System message level.
pub type SystemMessageLevel = String;

/// System message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMessage {
    #[serde(flatten)]
    pub base: MessageBase,
    #[serde(rename = "type")]
    pub message_type: String, // "system"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<SystemMessageLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// System local command message (subtype: "local_command").
pub type SystemLocalCommandMessage = SystemMessage;

/// System bridge status message.
pub type SystemBridgeStatusMessage = SystemMessage;

/// System turn duration message.
pub type SystemTurnDurationMessage = SystemMessage;

/// System thinking message.
pub type SystemThinkingMessage = SystemMessage;

/// System memory saved message.
pub type SystemMemorySavedMessage = SystemMessage;

/// System stop hook summary message.
pub type SystemStopHookSummaryMessage = SystemMessage;

/// System informational message.
pub type SystemInformationalMessage = SystemMessage;

/// System compact boundary message.
pub type SystemCompactBoundaryMessage = SystemMessage;

/// System micro-compact boundary message.
pub type SystemMicrocompactBoundaryMessage = SystemMessage;

/// System permission retry message.
pub type SystemPermissionRetryMessage = SystemMessage;

/// System scheduled task fire message.
pub type SystemScheduledTaskFireMessage = SystemMessage;

/// System away summary message.
pub type SystemAwaySummaryMessage = SystemMessage;

/// System agents killed message.
pub type SystemAgentsKilledMessage = SystemMessage;

/// System API metrics message.
pub type SystemApiMetricsMessage = SystemMessage;

/// System API error message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemApiErrorMessage {
    #[serde(flatten)]
    pub base: MessageBase,
    #[serde(rename = "type")]
    pub message_type: String, // "system"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<SystemMessageLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// System file snapshot message.
pub type SystemFileSnapshotMessage = SystemMessage;

/// Hook result message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResultMessage {
    #[serde(flatten)]
    pub base: MessageBase,
    #[serde(rename = "type")]
    pub message_type: String, // "hook_result"
}

/// Tool use summary message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseSummaryMessage {
    #[serde(flatten)]
    pub base: MessageBase,
    #[serde(rename = "type")]
    pub message_type: String, // "tool_use_summary"
}

/// Tombstone message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TombstoneMessage {
    #[serde(flatten)]
    pub base: MessageBase,
    #[serde(rename = "type")]
    pub message_type: String, // "tombstone"
}

/// Stream event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Request start event.
pub type RequestStartEvent = StreamEvent;

/// Stop hook info.
pub type StopHookInfo = HashMap<String, serde_json::Value>;

/// Compact metadata.
pub type CompactMetadata = HashMap<String, serde_json::Value>;

/// Partial compact direction.
pub type PartialCompactDirection = String;

/// Collapsed read search group.
pub type CollapsedReadSearchGroup = HashMap<String, serde_json::Value>;

/// Grouped tool use message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupedToolUseMessage {
    #[serde(flatten)]
    pub base: MessageBase,
    #[serde(rename = "type")]
    pub message_type: String, // "grouped_tool_use"
}

/// Collapsible message base.
pub type CollapsibleMessage = MessageBase;

/// Normalized assistant message.
pub type NormalizedAssistantMessage = AssistantMessage;

/// Normalized user message.
pub type NormalizedUserMessage = UserMessage;

/// Normalized message union.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NormalizedMessage {
    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),
    #[serde(rename = "user")]
    User(UserMessage),
    #[serde(rename = "progress")]
    Progress(ProgressMessage),
    #[serde(rename = "system")]
    System(SystemMessage),
    #[serde(rename = "attachment")]
    Attachment(AttachmentMessage),
}

/// Renderable message alias.
pub type RenderableMessage = Message;

/// Unified message enum covering all message variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    #[serde(rename = "user")]
    User(UserMessage),
    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),
    #[serde(rename = "progress")]
    Progress(ProgressMessage),
    #[serde(rename = "system")]
    System(SystemMessage),
    #[serde(rename = "attachment")]
    Attachment(AttachmentMessage),
    #[serde(rename = "hook_result")]
    HookResult(HookResultMessage),
    #[serde(rename = "tool_use_summary")]
    ToolUseSummary(ToolUseSummaryMessage),
    #[serde(rename = "tombstone")]
    Tombstone(TombstoneMessage),
    #[serde(rename = "grouped_tool_use")]
    GroupedToolUse(GroupedToolUseMessage),
}

/// Create an away summary system message.
/// Translates createAwaySummaryMessage from utils/messages.ts.
pub fn create_away_summary_message(content: &str) -> Message {
    Message::System(SystemMessage {
        base: MessageBase {
            uuid: Some(uuid::Uuid::new_v4().to_string()),
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
            is_meta: Some(false),
            ..Default::default()
        },
        message_type: "system".to_string(),
        subtype: Some("away_summary".to_string()),
        level: None,
        message: Some(content.to_string()),
    })
}
