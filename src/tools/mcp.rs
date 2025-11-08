use crate::tools::base::{Tool, ToolResult};
use async_trait::async_trait;
use once_cell::sync::OnceCell;
use rmcp::service::ServiceExt;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;

// Minimal MCP config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServersConfig {
    #[serde(rename = "mcpServers")]
    pub servers: HashMap<String, McpServer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub disabled: bool,
}

pub struct McpConnection {
    pub name: String,
    pub service: rmcp::service::RunningService<rmcp::service::RoleClient, ()>,
}

pub struct McpTool {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub service: Arc<rmcp::service::RunningService<rmcp::service::RoleClient, ()>>,
}

#[async_trait]
impl Tool for McpTool {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn parameters(&self) -> Value {
        self.parameters.clone()
    }
    async fn execute(&self, args: Value) -> ToolResult {
        // rmcp call_tool API expects Option<Map<String, Value>>
        let map = args.as_object().cloned();
        let param = rmcp::model::CallToolRequestParam {
            name: self.name.clone().into(),
            arguments: map,
        };
        let peer = self.service.peer();
        match peer.call_tool(param).await {
            Ok(result) => {
                // Flatten textual contents
                let mut parts = Vec::new();
                for c in result.content {
                    if let Some(t) = c.as_text() {
                        parts.push(t.text.clone());
                    } else {
                        parts.push(format!("{:?}", c.raw));
                    }
                }
                let text = parts.join("\n");
                let is_error = result.is_error.unwrap_or(false);
                ToolResult {
                    success: !is_error,
                    content: text,
                    error: if is_error {
                        Some("Tool returned error".into())
                    } else {
                        None
                    },
                }
            }
            Err(e) => ToolResult {
                success: false,
                content: String::new(),
                error: Some(e.to_string()),
            },
        }
    }
}

pub async fn load_mcp_tools(config_path: &Path) -> anyhow::Result<Vec<Arc<dyn Tool>>> {
    if !config_path.exists() {
        return Ok(Vec::new());
    }
    let cfg_text = tokio::fs::read_to_string(config_path).await?;
    let mcp_cfg: McpServersConfig = serde_json::from_str(&cfg_text)?;

    let mut tools: Vec<Arc<dyn Tool>> = Vec::new();

    for (name, server) in mcp_cfg.servers.into_iter() {
        if server.disabled {
            continue;
        }
        let mut cmd = Command::new(&server.command);
        for a in &server.args {
            cmd.arg(a);
        }
        for (k, v) in &server.env {
            cmd.env(k, v);
        }
        let transport = TokioChildProcess::new(cmd.configure(|_| {}))?;
        // The unit service () implements Service<RoleClient>
        let running = ().serve(transport).await?;
        let running = Arc::new(running);
        // store for cleanup
        REGISTRY
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .await
            .push(running.clone());
        let info = running.peer_info();
        tracing::info!(server = %name, ?info, "Connected MCP server");

        // list tools
        let list = running.peer().list_tools(Default::default()).await?;
        for t in list.tools {
            let params = t.schema_as_json_value();
            let tool = McpTool {
                name: t.name.to_string(),
                description: t.description.unwrap_or_default().to_string(),
                parameters: params,
                service: running.clone(),
            };
            tools.push(Arc::new(tool));
        }
    }

    Ok(tools)
}

// Global registry to cleanup MCP connections
static REGISTRY: OnceCell<
    Mutex<Vec<Arc<rmcp::service::RunningService<rmcp::service::RoleClient, ()>>>>,
> = OnceCell::new();

pub async fn cleanup_mcp() {
    if let Some(reg) = REGISTRY.get() {
        let conns = reg.lock().await;
        for rs in conns.iter() {
            rs.cancellation_token().cancel();
        }
    }
}
