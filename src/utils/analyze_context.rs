// Source: ~/claudecode/openclaudecode/src/utils/analyzeContext.ts
use std::collections::HashMap;

use crate::types::{Message, MessageRole};
use crate::services::token_estimation::rough_token_count_estimation_for_content;

/// Per-category token breakdown
#[derive(Debug, Clone, Default)]
pub struct TokenStats {
    pub tool_requests: HashMap<String, u64>,
    pub tool_results: HashMap<String, u64>,
    pub human_messages: u64,
    pub assistant_messages: u64,
    pub local_command_outputs: u64,
    pub other: u64,
    pub attachments: HashMap<String, u64>,
    pub duplicate_file_reads: HashMap<String, FileReadStats>,
    pub total: u64,
}

#[derive(Debug, Clone, Default)]
pub struct FileReadStats {
    pub count: u64,
    pub tokens: u64,
}

/// Analyze the token distribution across message categories.
pub fn analyze_context(messages: &[Message]) -> TokenStats {
    let mut stats = TokenStats::default();
    let mut tool_ids_to_names: HashMap<String, String> = HashMap::new();
    let mut read_tool_id_to_file_path: HashMap<String, String> = HashMap::new();
    let mut seen_file_paths: HashMap<String, u64> = HashMap::new();

    for msg in messages {
        match msg.role {
            MessageRole::User => {
                let tokens = rough_token_count_estimation_for_content(&msg.content) as u64;
                stats.human_messages += tokens;
                stats.total += tokens;
            }
            MessageRole::Assistant => {
                let tokens = rough_token_count_estimation_for_content(&msg.content) as u64;
                stats.assistant_messages += tokens;
                stats.total += tokens;

                // Track tool calls for result attribution
                if let Some(ref tool_calls) = msg.tool_calls {
                    for call in tool_calls {
                        tool_ids_to_names.insert(call.id.clone(), call.name.clone());

                        // Track Read tool file paths for duplicate detection
                        if call.name == "Read" {
                            if let Some(path) = call.arguments.get("file_path").and_then(|p| p.as_str()) {
                                read_tool_id_to_file_path
                                    .insert(call.id.clone(), path.to_string());

                                let tokens = rough_token_count_estimation_for_content(&msg.content) as u64;
                                let entry = seen_file_paths.entry(path.to_string()).or_insert(0);
                                *entry += 1;
                                if *entry > 1 {
                                    stats.duplicate_file_reads.insert(
                                        path.to_string(),
                                        FileReadStats {
                                            count: *entry,
                                            tokens,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }
            MessageRole::Tool => {
                let tokens = rough_token_count_estimation_for_content(&msg.content) as u64;
                stats.total += tokens;

                if let Some(ref tool_call_id) = msg.tool_call_id {
                    if let Some(tool_name) = tool_ids_to_names.get(tool_call_id) {
                        *stats.tool_results.entry(tool_name.clone()).or_insert(0) += tokens;

                        // Track tool request tokens too
                        *stats.tool_requests.entry(tool_name.clone()).or_insert(0) += 10; // overhead estimate
                    } else {
                        stats.local_command_outputs += tokens;
                    }
                } else {
                    stats.other += tokens;
                }
            }
            MessageRole::System => {
                let tokens = rough_token_count_estimation_for_content(&msg.content) as u64;
                stats.other += tokens;
                stats.total += tokens;
            }
        }

        // Count attachment tokens
        if let Some(ref attachments) = msg.attachments {
            for attachment in attachments {
                let attachment_name = match attachment {
                    crate::types::Attachment::File { path } => "File".to_string(),
                    crate::types::Attachment::AlreadyReadFile { path, .. } => "AlreadyReadFile".to_string(),
                    crate::types::Attachment::PdfReference { .. } => "PdfReference".to_string(),
                    crate::types::Attachment::EditedTextFile { .. } => "EditedTextFile".to_string(),
                    crate::types::Attachment::EditedImageFile { .. } => "EditedImageFile".to_string(),
                    crate::types::Attachment::Directory { .. } => "Directory".to_string(),
                    crate::types::Attachment::SelectedLinesInIde { .. } => "SelectedLinesInIde".to_string(),
                    crate::types::Attachment::MemoryFile { .. } => "MemoryFile".to_string(),
                    crate::types::Attachment::SkillListing { .. } => "SkillListing".to_string(),
                    crate::types::Attachment::InvokedSkills { .. } => "InvokedSkills".to_string(),
                    crate::types::Attachment::TaskStatus { .. } => "TaskStatus".to_string(),
                    crate::types::Attachment::PlanFileReference { .. } => "PlanFileReference".to_string(),
                    crate::types::Attachment::McpResources { .. } => "McpResources".to_string(),
                    crate::types::Attachment::DeferredTools { .. } => "DeferredTools".to_string(),
                    crate::types::Attachment::AgentListing { .. } => "AgentListing".to_string(),
                    crate::types::Attachment::Custom { name, .. } => name.clone(),
                };
                let attachment_tokens = serde_json::to_string(attachment)
                    .map(|s| rough_token_count_estimation_for_content(&s) as u64)
                    .unwrap_or(0);
                *stats.attachments.entry(attachment_name).or_insert(0) += attachment_tokens;
            }
        }
    }

    stats
}

/// Convert TokenStats to a flat metrics map for analytics.
pub fn token_stats_to_metrics(stats: &TokenStats) -> HashMap<String, f64> {
    let mut metrics = HashMap::new();
    let total = if stats.total > 0 { stats.total as f64 } else { 1.0 };

    metrics.insert("tool_requests_count".to_string(), stats.tool_requests.len() as f64);
    metrics.insert("tool_results_count".to_string(), stats.tool_results.len() as f64);
    metrics.insert(
        "human_messages_pct".to_string(),
        stats.human_messages as f64 / total * 100.0,
    );
    metrics.insert(
        "assistant_messages_pct".to_string(),
        stats.assistant_messages as f64 / total * 100.0,
    );
    metrics.insert(
        "local_command_outputs_pct".to_string(),
        stats.local_command_outputs as f64 / total * 100.0,
    );
    metrics.insert(
        "other_pct".to_string(),
        stats.other as f64 / total * 100.0,
    );
    metrics.insert("attachments_count".to_string(), stats.attachments.len() as f64);
    metrics.insert(
        "duplicate_file_reads_count".to_string(),
        stats.duplicate_file_reads.len() as f64,
    );
    metrics.insert("total_tokens".to_string(), stats.total as f64);

    metrics
}

/// Analyze context usage and return as JSON (for tool/plugin use).
pub async fn analyze_context_usage() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "message": "Use analyze_context() directly with messages",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_context_empty() {
        let stats = analyze_context(&[]);
        assert_eq!(stats.total, 0);
    }

    #[test]
    fn test_analyze_context_user_message() {
        let messages = vec![Message {
            role: MessageRole::User,
            content: "Hello world".to_string(),
            ..Default::default()
        }];
        let stats = analyze_context(&messages);
        assert!(stats.human_messages > 0);
        assert_eq!(stats.total, stats.human_messages);
    }

    #[test]
    fn test_analyze_context_assistant_message() {
        let messages = vec![Message {
            role: MessageRole::Assistant,
            content: "Here's the answer".to_string(),
            ..Default::default()
        }];
        let stats = analyze_context(&messages);
        assert!(stats.assistant_messages > 0);
    }

    #[test]
    fn test_token_stats_to_metrics() {
        let mut stats = TokenStats::default();
        stats.total = 1000;
        stats.human_messages = 300;
        let metrics = token_stats_to_metrics(&stats);
        assert!((metrics.get("human_messages_pct").unwrap() - 30.0) < 0.1);
    }
}
