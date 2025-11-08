use crate::tools::base::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Stdio;

pub struct BashTool {
    pub workspace: PathBuf,
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }
    fn description(&self) -> &str {
        "Execute a shell command in the workspace"
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "Command to run"}
            },
            "required": ["command"],
        })
    }
    async fn execute(&self, args: Value) -> ToolResult {
        let Some(cmd) = args.get("command").and_then(|v| v.as_str()) else {
            return ToolResult {
                success: false,
                content: String::new(),
                error: Some("missing 'command'".into()),
            };
        };

        #[cfg(target_os = "windows")]
        let mut command = tokio::process::Command::new("cmd");
        #[cfg(target_os = "windows")]
        let command = command.arg("/C").arg(cmd).current_dir(&self.workspace);

        #[cfg(not(target_os = "windows"))]
        let mut command = tokio::process::Command::new("bash");
        #[cfg(not(target_os = "windows"))]
        let command = command.arg("-lc").arg(cmd).current_dir(&self.workspace);

        let output = match command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
        {
            Ok(o) => o,
            Err(e) => {
                return ToolResult {
                    success: false,
                    content: String::new(),
                    error: Some(e.to_string()),
                };
            }
        };
        let mut content = String::new();
        if !output.stdout.is_empty() {
            content.push_str(&String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            content.push_str(&String::from_utf8_lossy(&output.stderr));
        }
        ToolResult {
            success: output.status.success(),
            content,
            error: if output.status.success() {
                None
            } else {
                Some(format!("exit: {}", output.status))
            },
        }
    }
}
