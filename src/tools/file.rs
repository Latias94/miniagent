use crate::tools::base::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn schema_for_path_content() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": {"type": "string", "description": "Relative or absolute file path"},
            "content": {"type": "string", "description": "File content (UTF-8)"}
        },
        "required": ["path"],
    })
}

fn resolve_path(workspace: &Path, input: &str) -> PathBuf {
    let path = PathBuf::from(input);
    if path.is_absolute() {
        path
    } else {
        workspace.join(path)
    }
}

pub struct ReadTool {
    pub workspace: PathBuf,
}
pub struct WriteTool {
    pub workspace: PathBuf,
}
pub struct EditTool {
    pub workspace: PathBuf,
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read_file"
    }
    fn description(&self) -> &str {
        "Read a text file from workspace (UTF-8)"
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "path": {"type": "string"} },
            "required": ["path"],
        })
    }

    async fn execute(&self, args: Value) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => {
                return ToolResult {
                    success: false,
                    content: String::new(),
                    error: Some("missing 'path'".into()),
                };
            }
        };
        let full = resolve_path(&self.workspace, path);
        match tokio::fs::read_to_string(&full).await {
            Ok(c) => ToolResult {
                success: true,
                content: c,
                error: None,
            },
            Err(e) => ToolResult {
                success: false,
                content: String::new(),
                error: Some(format!("read error: {}", e)),
            },
        }
    }
}

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write_file"
    }
    fn description(&self) -> &str {
        "Write text to a file (create/overwrite, UTF-8)"
    }
    fn parameters(&self) -> Value {
        schema_for_path_content()
    }
    async fn execute(&self, args: Value) -> ToolResult {
        let path = args.get("path").and_then(|v| v.as_str());
        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let Some(p) = path else {
            return ToolResult {
                success: false,
                content: String::new(),
                error: Some("missing 'path'".into()),
            };
        };
        let full = resolve_path(&self.workspace, p);
        if let Some(parent) = full.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        match tokio::fs::write(&full, content).await {
            Ok(_) => ToolResult {
                success: true,
                content: format!("wrote {} bytes to {}", content.len(), full.display()),
                error: None,
            },
            Err(e) => ToolResult {
                success: false,
                content: String::new(),
                error: Some(format!("write error: {}", e)),
            },
        }
    }
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit_file"
    }
    fn description(&self) -> &str {
        "Search and replace text within a file"
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "old_str": {"type": "string"},
                "new_str": {"type": "string"}
            },
            "required": ["path", "old_str", "new_str"],
        })
    }
    async fn execute(&self, args: Value) -> ToolResult {
        let Some(path) = args.get("path").and_then(|v| v.as_str()) else {
            return ToolResult {
                success: false,
                content: String::new(),
                error: Some("missing 'path'".into()),
            };
        };
        let Some(search) = args.get("old_str").and_then(|v| v.as_str()) else {
            return ToolResult {
                success: false,
                content: String::new(),
                error: Some("missing 'old_str'".into()),
            };
        };
        let Some(replace) = args.get("new_str").and_then(|v| v.as_str()) else {
            return ToolResult {
                success: false,
                content: String::new(),
                error: Some("missing 'new_str'".into()),
            };
        };
        let full = resolve_path(&self.workspace, path);
        let Ok(mut content) = tokio::fs::read_to_string(&full).await else {
            return ToolResult {
                success: false,
                content: String::new(),
                error: Some(format!("read error: {}", full.display())),
            };
        };
        let count = content.matches(search).count();
        content = content.replace(search, replace);
        match tokio::fs::write(&full, &content).await {
            Ok(_) => ToolResult {
                success: true,
                content: format!("replaced {} occurrence(s) in {}", count, full.display()),
                error: None,
            },
            Err(e) => ToolResult {
                success: false,
                content: String::new(),
                error: Some(format!("write error: {}", e)),
            },
        }
    }
}
