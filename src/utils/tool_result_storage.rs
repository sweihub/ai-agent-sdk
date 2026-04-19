use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub struct ToolResultStorage {
    results: Arc<RwLock<HashMap<String, ToolResult>>>,
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_name: String,
    pub input: serde_json::Value,
    pub output: serde_json::Value,
    pub timestamp: u64,
    was_persisted: None,
}

impl ToolResultStorage {
    pub fn new() -> Self {
        Self {
            results: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn store(&self, id: String, result: ToolResult) -> Result<(), String> {
        let mut results = self.results.write().map_err(|e| e.to_string())?;
        results.insert(id, result);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<ToolResult>, String> {
        let results = self.results.read().map_err(|e| e.to_string())?;
        Ok(results.get(id).cloned())
    }

    pub fn delete(&self, id: &str) -> Result<Option<ToolResult>, String> {
        let mut results = self.results.write().map_err(|e| e.to_string())?;
        Ok(results.remove(id))
    }

    pub fn list_by_tool(&self, tool_name: &str) -> Result<Vec<ToolResult>, String> {
        let results = self.results.read().map_err(|e| e.to_string())?;
        Ok(results
            .values()
            .filter(|r| r.tool_name == tool_name)
            .cloned()
            .collect())
    }

    pub fn clear(&self) -> Result<(), String> {
        let mut results = self.results.write().map_err(|e| e.to_string())?;
        results.clear();
        Ok(())
    }
}

impl Default for ToolResultStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage() {
        let storage = ToolResultStorage::new();

        let result = ToolResult {
            tool_name: "bash".to_string(),
            input: serde_json::json!({"command": "ls"}),
            output: serde_json::json!({"stdout": "file.txt"}),
            timestamp: 123456,
            was_persisted: None,
        };

        storage.store("1".to_string(), result).unwrap();
        let retrieved = storage.get("1").unwrap().unwrap();
        assert_eq!(retrieved.tool_name, "bash");
    }
}
