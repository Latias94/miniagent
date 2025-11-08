use crate::tools::base::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::path::PathBuf;

pub struct RecordNoteTool {
    pub memory_file: PathBuf,
}
pub struct RecallNotesTool {
    pub memory_file: PathBuf,
}

fn load_notes(path: &PathBuf) -> Vec<serde_json::Value> {
    if !path.exists() {
        return vec![];
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| vec![])
}

fn save_notes(path: &PathBuf, notes: &[serde_json::Value]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        serde_json::to_string_pretty(notes).unwrap_or_default(),
    )
}

#[async_trait]
impl Tool for RecordNoteTool {
    fn name(&self) -> &str {
        "record_note"
    }
    fn description(&self) -> &str {
        "Record important information as session notes for future reference (timestamped)."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "content": {"type": "string", "description": "Note content"},
                "category": {"type": "string", "description": "Optional category"}
            },
            "required": ["content"],
        })
    }

    async fn execute(&self, args: Value) -> ToolResult {
        let Some(content) = args.get("content").and_then(|v| v.as_str()) else {
            return ToolResult {
                success: false,
                content: String::new(),
                error: Some("missing 'content'".into()),
            };
        };
        let category = args
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("general");
        let mut notes = load_notes(&self.memory_file);
        notes.push(json!({
            "timestamp": chrono::Local::now().to_rfc3339(),
            "category": category,
            "content": content,
        }));
        match save_notes(&self.memory_file, &notes) {
            Ok(_) => ToolResult {
                success: true,
                content: format!("Recorded note: {} (category: {})", content, category),
                error: None,
            },
            Err(e) => ToolResult {
                success: false,
                content: String::new(),
                error: Some(format!("Failed to record note: {}", e)),
            },
        }
    }
}

#[async_trait]
impl Tool for RecallNotesTool {
    fn name(&self) -> &str {
        "recall_notes"
    }
    fn description(&self) -> &str {
        "Recall all previously recorded session notes (optionally filter by category)."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "category": {"type": "string", "description": "Optional category filter"}
            }
        })
    }

    async fn execute(&self, args: Value) -> ToolResult {
        let notes = load_notes(&self.memory_file);
        if notes.is_empty() {
            return ToolResult {
                success: true,
                content: "No notes recorded yet.".into(),
                error: None,
            };
        }
        let filter = args.get("category").and_then(|v| v.as_str());
        let filtered: Vec<_> = notes
            .into_iter()
            .filter(|n| match filter {
                Some(cat) => n.get("category").and_then(|v| v.as_str()) == Some(cat),
                None => true,
            })
            .collect();
        if filtered.is_empty() {
            return ToolResult {
                success: true,
                content: format!(
                    "No notes found{}",
                    filter
                        .map(|c| format!(" in category: {}", c))
                        .unwrap_or_default()
                ),
                error: None,
            };
        }
        let mut out = String::from("Recorded Notes:\n");
        for (idx, n) in filtered.iter().enumerate() {
            let ts = n
                .get("timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown time");
            let cat = n
                .get("category")
                .and_then(|v| v.as_str())
                .unwrap_or("general");
            let ct = n.get("content").and_then(|v| v.as_str()).unwrap_or("");
            out.push_str(&format!(
                "{}. [{}] {}\n   (recorded at {})\n",
                idx + 1,
                cat,
                ct,
                ts
            ));
        }
        ToolResult {
            success: true,
            content: out,
            error: None,
        }
    }
}
