// Source: /data/home/swei/claudecode/openclaudecode/src/memdir/findRelevantMemories.ts
//! Find relevant memories using LLM-based selection.

use std::path::Path;

use crate::memdir::memory_scan::{MemoryHeader, scan_memory_files};
use crate::utils::side_query::{SideQueryMemorySelection, SideQueryOptions, side_query};

/// System prompt for LLM-based memory selection.
const SELECT_MEMORIES_SYSTEM_PROMPT: &str =
    "You are a memory selection assistant. Given a query and a list of available memory files, \
     select the files most likely to contain relevant information for answering the query. \
     Return your selection as JSON with a 'filenames' array and 'reasoning' string. \
     Only select files that are genuinely relevant. If none are relevant, return an empty array.";

/// Maximum number of memories to consider for selection.
const MAX_CANDIDATES: usize = 50;

/// A relevant memory entry returned by the selection process.
#[derive(Debug, Clone)]
pub struct RelevantMemory {
    pub path: String,
    pub mtime_ms: u64,
}

/// Configuration for find_relevant_memories.
pub struct FindRelevantMemoriesConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_candidates: Option<usize>,
}

impl Default for FindRelevantMemoriesConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.anthropic.com".to_string(),
            api_key: String::new(),
            model: "claude-haiku-4-6".to_string(),
            max_candidates: None,
        }
    }
}

/// Find relevant memories for a given query.
pub async fn find_relevant_memories(query: &str, memory_dir: &Path) -> Vec<String> {
    find_relevant_memories_with_config(query, memory_dir, &Default::default()).await
}

/// Find relevant memories with custom configuration.
pub async fn find_relevant_memories_with_config(
    query: &str,
    memory_dir: &Path,
    config: &FindRelevantMemoriesConfig,
) -> Vec<String> {
    let dir_str = match memory_dir.to_str() {
        Some(s) => s.to_string(),
        None => return Vec::new(),
    };

    let memory_files = scan_memory_files(&dir_str).await;

    if memory_files.is_empty() {
        return Vec::new();
    }

    let max_candidates = config.max_candidates.unwrap_or(MAX_CANDIDATES);
    let candidates: Vec<&MemoryHeader> = memory_files.iter().take(max_candidates).collect();

    if candidates.is_empty() {
        return Vec::new();
    }

    let selected = select_relevant_memories(query, &candidates, config).await;

    let selected_set: std::collections::HashSet<&str> =
        selected.filenames.iter().map(|s| s.as_str()).collect();

    let mut results = Vec::new();
    for mem in &memory_files {
        if selected_set.contains(mem.filename.as_str()) {
            results.push(mem.file_path.clone());
        }
    }
    results
}

/// Use LLM to select relevant memories from candidates.
async fn select_relevant_memories(
    query: &str,
    candidates: &[&MemoryHeader],
    config: &FindRelevantMemoriesConfig,
) -> SideQueryMemorySelection {
    let candidate_list: Vec<String> = candidates
        .iter()
        .map(|mem| {
            let name = mem.filename.as_str();
            let description = mem.description.as_deref().unwrap_or("");
            let mem_type = mem.memory_type.as_ref().map_or("memory", |t| t.as_str());
            let mtime = mem.mtime_ms;
            format!(
                "- {} (type: {}, modified: {}, description: {})",
                name, mem_type, mtime, description
            )
        })
        .collect();

    let message = format!(
        "Query: {}\n\nAvailable memory files:\n{}\n\nSelect the most relevant files.",
        query,
        candidate_list.join("\n")
    );

    if !config.api_key.is_empty() {
        let opts = SideQueryOptions::new(
            config.base_url.clone(),
            config.api_key.clone(),
            config.model.clone(),
        )
        .system_prompt(SELECT_MEMORIES_SYSTEM_PROMPT.to_string())
        .message(message)
        .max_tokens(2048);

        if let Ok(response) = side_query(&opts).await {
            let selection = SideQueryMemorySelection::from_response(&response);
            if !selection.filenames.is_empty() {
                return selection;
            }
        }
    }

    // Fallback: return all candidates
    SideQueryMemorySelection {
        filenames: candidates
            .iter()
            .map(|mem| mem.filename.clone())
            .collect(),
        reasoning: "LLM selection unavailable, returning all candidates".to_string(),
    }
}

/// Extract filenames from an arbitrary text response.
pub fn extract_filenames_from_text(text: &str) -> Vec<String> {
    let mut filenames = Vec::new();
    for line in text.lines() {
        let clean = line.trim()
            .trim_start_matches('-')
            .trim_start_matches('*')
            .trim_start_matches('`')
            .trim_end_matches('`')
            .trim()
            .to_string();
        if clean.is_empty() || filenames.contains(&clean) {
            continue;
        }
        if clean.ends_with(".md")
            || clean.ends_with(".txt")
            || clean.ends_with(".json")
        {
            filenames.push(clean);
        }
    }
    filenames
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_relevant_memories_empty_directory() {
        let temp = tempfile::tempdir().unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let paths = rt.block_on(find_relevant_memories("test query", temp.path()));
        assert!(paths.is_empty());
    }

    #[test]
    fn test_find_relevant_memories_nonexistent_directory() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let paths =
            rt.block_on(find_relevant_memories("test", Path::new("/nonexistent/path")));
        assert!(paths.is_empty());
    }

    #[test]
    fn test_extract_filenames_from_text() {
        let text = "- memory.md\n* notes.txt\nconfig.json\nnot a file\n";
        let filenames = extract_filenames_from_text(text);
        assert_eq!(filenames.len(), 3);
        assert!(filenames.contains(&"memory.md".to_string()));
    }

    #[test]
    fn test_relevant_memory_struct() {
        let mem = RelevantMemory {
            path: "/tmp/test.md".to_string(),
            mtime_ms: 1234567890,
        };
        assert_eq!(mem.path, "/tmp/test.md");
    }
}
