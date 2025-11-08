use async_trait::async_trait;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub success: bool,
    pub content: String,
    pub error: Option<String>,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    async fn execute(&self, args: Value) -> ToolResult;

    fn to_siumai_tool(&self) -> siumai::types::Tool {
        siumai::types::Tool::function(
            self.name().to_string(),
            self.description().to_string(),
            self.parameters(),
        )
    }
}
