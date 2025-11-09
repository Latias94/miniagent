use crate::config::RetryConfig;
use crate::llm::LlmClient;
use crate::logger::AgentLogger;
use crate::observer::{AgentObserver, ConsoleObserver};
#[cfg(not(feature = "tiktoken"))]
use crate::token::ApproxEstimator;
use crate::token::TokenEstimator;
use crate::tools::{Tool, base::ToolResult};
use colored::*;
use serde_json::json;
use siumai::traits::ChatCapability;
use siumai::types::{ChatMessage, ChatRequest, ContentPart, MessageContent, Tool as SiumaiTool};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub struct Agent {
    llm: LlmClient,
    tools: HashMap<String, Arc<dyn Tool>>,
    pub messages: Vec<ChatMessage>,
    pub max_steps: usize,
    pub token_limit: usize,
    pub completion_reserve: usize,
    pub workspace: PathBuf,
    logger: AgentLogger,
    estimator: Box<dyn TokenEstimator>,
    retry: RetryConfig,
    observer: Arc<dyn AgentObserver>,
}

impl Agent {
    pub fn new(
        llm: LlmClient,
        system_prompt: String,
        tools: Vec<Arc<dyn Tool>>,
        max_steps: usize,
        token_limit: usize,
        completion_reserve: usize,
        workspace_dir: PathBuf,
        retry: RetryConfig,
    ) -> Self {
        let mut msg = Vec::new();
        msg.push(ChatMessage::system(system_prompt).build());
        let mut map = HashMap::new();
        for t in tools {
            map.insert(t.name().to_string(), t);
        }
        #[cfg(feature = "tiktoken")]
        let estimator: Box<dyn TokenEstimator> =
            Box::new(crate::token::TiktokenEstimator::cl100k());
        #[cfg(not(feature = "tiktoken"))]
        let estimator: Box<dyn TokenEstimator> = Box::new(ApproxEstimator);
        Self {
            llm,
            tools: map,
            messages: msg,
            max_steps,
            token_limit,
            completion_reserve,
            workspace: workspace_dir,
            logger: AgentLogger::new(),
            estimator,
            retry,
            observer: Arc::new(ConsoleObserver::new()),
        }
    }

    pub fn add_user_message(&mut self, text: String) {
        self.messages.push(ChatMessage::user(text).build());
    }

    fn to_siumai_tools(&self) -> Vec<SiumaiTool> {
        self.tools.values().map(|t| t.to_siumai_tool()).collect()
    }

    pub fn tool_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tools.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn tool_schema(&self, name: &str) -> Option<(String, String, serde_json::Value)> {
        self.tools.get(name).map(|t| {
            (
                t.name().to_string(),
                t.description().to_string(),
                t.parameters(),
            )
        })
    }

    pub async fn call_tool_direct(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Option<ToolResult> {
        let tool = self.tools.get(name).cloned();
        match tool {
            Some(t) => Some(t.execute(args).await),
            None => None,
        }
    }

    pub fn set_observer(&mut self, obs: Arc<dyn AgentObserver>) {
        self.observer = obs;
    }

    pub fn builder(llm: LlmClient, system_prompt: String) -> AgentBuilder {
        AgentBuilder::new(llm, system_prompt)
    }

    pub async fn run(&mut self) -> anyhow::Result<String> {
        self.logger.start_new_run();
        if let Some(p) = self.logger.log_path() {
            self.observer.on_log_file(p);
        }

        let mut step = 0usize;
        loop {
            // summarize if tokens exceed limit
            let threshold = self.token_limit.saturating_sub(self.completion_reserve);
            if self.estimator.count_messages(&self.messages) > threshold {
                self.summarize_history().await?;
            }
            if step >= self.max_steps {
                let msg = format!("Task couldn't be completed after {} steps.", self.max_steps);
                return Ok(msg);
            }

            // Log request
            let tools_schema: Vec<_> = self.to_siumai_tools().into_iter().collect();
            let req_json = json!({
                "messages": self.messages.iter().map(|m| {
                    json!({"role": format!("{:?}", m.role), "content": m.content_text().unwrap_or("")})
                }).collect::<Vec<_>>(),
                "tools": tools_schema.iter().map(|t| match t { SiumaiTool::Function { function } => function.name.clone(), _ => String::from("provider_tool")}).collect::<Vec<_>>()
            });
            self.logger.log_request(&req_json);

            // Call LLM (built-in retry is configured on the Siumai client via builder)
            let tools_vec = self
                .tools
                .values()
                .map(|t| t.to_siumai_tool())
                .collect::<Vec<_>>();
            let req = ChatRequest::new(self.messages.clone()).with_tools(tools_vec);
            let response = self.llm.inner().chat_request(req).await?;

            // Log response
            let resp_json = json!({
                "content": response.content_text(),
                "has_tool_calls": response.has_tool_calls(),
                "finish_reason": response.finish_reason,
            });
            self.logger.log_response(&resp_json);

            // Append assistant message to history
            match &response.content {
                MessageContent::Text(t) => {
                    if !t.is_empty() {
                        self.observer.on_assistant_text(t);
                    }
                    self.messages
                        .push(ChatMessage::assistant(t.clone()).build());
                }
                MessageContent::MultiModal(parts) => {
                    // Print reasoning if present
                    for r in response.reasoning() {
                        self.observer.on_thinking(r);
                    }
                    // Print assistant text parts
                    for p in parts {
                        if let ContentPart::Text { text } = p {
                            if !text.is_empty() {
                                self.observer.on_assistant_text(text);
                            }
                        }
                    }
                    self.messages
                        .push(ChatMessage::assistant_with_content(parts.clone()).build());
                }
            }

            // If no tool calls, return content text
            if !response.has_tool_calls() {
                return Ok(response.content_text().unwrap_or("").to_string());
            }

            // Execute tool calls
            for call in response.tool_calls() {
                if let Some(info) = call.as_tool_call() {
                    let tool_name = info.tool_name.to_string();
                    let args = info.arguments.clone();
                    // Truncate each argument value recursively for display purposes
                    fn truncate_value(v: &serde_json::Value) -> serde_json::Value {
                        use serde_json::Value::*;
                        match v {
                            String(s) => {
                                if s.len() > 200 {
                                    String(format!("{}...", &s[..200]))
                                } else {
                                    String(s.clone())
                                }
                            }
                            Array(a) => Array(a.iter().map(truncate_value).collect()),
                            Object(m) => {
                                let mut o = serde_json::Map::new();
                                for (k, vv) in m.iter() {
                                    o.insert(k.clone(), truncate_value(vv));
                                }
                                Object(o)
                            }
                            other => other.clone(),
                        }
                    }
                    let display_args =
                        serde_json::to_string_pretty(&truncate_value(&args)).unwrap_or_default();
                    self.observer.on_tool_call(&tool_name, &display_args);

                    let result: ToolResult = match self.tools.get(&tool_name) {
                        Some(t) => t.execute(args.clone()).await,
                        None => ToolResult {
                            success: false,
                            content: String::new(),
                            error: Some(format!("Unknown tool: {}", tool_name)),
                        },
                    };

                    // Log tool result
                    let payload = json!({
                        "tool_name": tool_name,
                        "arguments": args,
                        "success": result.success,
                        "result": if result.success { Some(result.content.clone()) } else { None::<String> },
                        "error": result.error,
                    });
                    self.logger.log_tool_result(&payload);

                    // Print and append tool result message
                    if result.success {
                        let preview = if result.content.len() > 300 {
                            format!("{}...", &result.content[..300])
                        } else {
                            result.content.clone()
                        };
                        self.observer.on_tool_result(&tool_name, true, &preview);
                        self.messages.push(
                            ChatMessage::tool_result_text(
                                info.tool_call_id,
                                tool_name,
                                result.content,
                            )
                            .build(),
                        );
                    } else {
                        let err = result
                            .error
                            .unwrap_or_else(|| "Tool execution failed".to_string());
                        self.observer.on_tool_result(&tool_name, false, &err);
                        self.messages.push(
                            ChatMessage::tool_error(info.tool_call_id, tool_name, err).build(),
                        );
                    }
                }
            }

            step += 1;
        }
    }

    async fn summarize_history(&mut self) -> anyhow::Result<()> {
        // strategy: keep system + every user; summarize assistant/tool blocks between user pairs
        let before = self.estimator.count_messages(&self.messages);
        let mut new_msgs = Vec::<ChatMessage>::new();
        if let Some(first) = self.messages.first().cloned() {
            new_msgs.push(first);
        }

        // collect indices of user messages beyond system
        let mut user_idxs = Vec::new();
        for (i, m) in self.messages.iter().enumerate().skip(1) {
            if matches!(m.role, siumai::types::MessageRole::User) {
                user_idxs.push(i);
            }
        }
        if user_idxs.is_empty() {
            return Ok(());
        }
        for (pos, &u_idx) in user_idxs.iter().enumerate() {
            new_msgs.push(self.messages[u_idx].clone());
            let end = user_idxs
                .get(pos + 1)
                .cloned()
                .unwrap_or(self.messages.len());
            let segment = &self.messages[u_idx + 1..end];
            if !segment.is_empty() {
                let summary = self
                    .create_summary(segment, pos + 1)
                    .await
                    .unwrap_or_else(|_| String::new());
                let content = format!("[Assistant Execution Summary]\n\n{}", summary);
                new_msgs.push(ChatMessage::user(content).build());
            }
        }
        self.messages = new_msgs;
        let after = self.estimator.count_messages(&self.messages);
        let threshold = self.token_limit;
        self.observer.on_summarize_start(before, threshold);
        self.observer.on_summarize_done(after);
        Ok(())
    }

    async fn create_summary(
        &self,
        messages: &[ChatMessage],
        round: usize,
    ) -> anyhow::Result<String> {
        // build plain text
        let mut buf = String::new();
        buf.push_str(&format!("Round {} execution process:\n\n", round));
        for m in messages {
            match m.role {
                siumai::types::MessageRole::Assistant => match &m.content {
                    MessageContent::Text(t) => buf.push_str(&format!("Assistant: {}\n", t)),
                    MessageContent::MultiModal(parts) => {
                        let text = parts
                            .iter()
                            .filter_map(|p| {
                                if let ContentPart::Text { text } = p {
                                    Some(text.as_str())
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(" ");
                        if !text.is_empty() {
                            buf.push_str(&format!("Assistant: {}\n", text));
                        }
                        let tool_names: Vec<_> = parts
                            .iter()
                            .filter_map(|p| {
                                if let ContentPart::ToolCall { tool_name, .. } = p {
                                    Some(tool_name.as_str())
                                } else {
                                    None
                                }
                            })
                            .collect();
                        if !tool_names.is_empty() {
                            buf.push_str(&format!(
                                "  -> Called tools: {}\n",
                                tool_names.join(", ")
                            ));
                        }
                    }
                },
                siumai::types::MessageRole::Tool => match &m.content {
                    MessageContent::MultiModal(parts) => {
                        let preview = parts
                            .iter()
                            .filter_map(|p| {
                                if let ContentPart::ToolResult { .. } = p {
                                    Some("[tool-result]")
                                } else {
                                    None
                                }
                            })
                            .count();
                        buf.push_str(&format!("  -> Tool returned: {} result(s)\n", preview));
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        let prompt = format!(
            concat!(
                "Please provide a concise summary of the following Agent execution process:\n\n",
                "{}\n\n",
                "Requirements:\n",
                "1. Focus on what tasks were completed and which tools were called\n",
                "2. Keep key execution results and important findings\n",
                "3. Be concise and clear, within 1000 words\n",
                "4. Use English\n",
                "5. Do not include user content, only summarize the Agent's execution process\n"
            ),
            buf
        );
        let req = vec![
            ChatMessage::system(
                "You are an assistant skilled at summarizing Agent execution processes.",
            )
            .build(),
            ChatMessage::user(prompt).build(),
        ];
        let resp = self.llm.inner().chat(req).await?;
        Ok(resp.content_text().unwrap_or("").to_string())
    }
}

pub struct AgentBuilder {
    llm: LlmClient,
    system_prompt: String,
    tools: Vec<Arc<dyn Tool>>,
    max_steps: usize,
    token_limit: usize,
    completion_reserve: usize,
    workspace: PathBuf,
    retry: RetryConfig,
    observer: Arc<dyn AgentObserver>,
}

impl AgentBuilder {
    pub fn new(llm: LlmClient, system_prompt: String) -> Self {
        Self {
            llm,
            system_prompt,
            tools: Vec::new(),
            max_steps: 50,
            token_limit: 80_000,
            completion_reserve: 2_048,
            workspace: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            retry: RetryConfig::default(),
            observer: Arc::new(ConsoleObserver::new()),
        }
    }

    pub fn with_tools(mut self, tools: Vec<Arc<dyn Tool>>) -> Self {
        self.tools = tools;
        self
    }
    pub fn add_tool(mut self, tool: Arc<dyn Tool>) -> Self {
        self.tools.push(tool);
        self
    }
    pub fn with_max_steps(mut self, v: usize) -> Self {
        self.max_steps = v;
        self
    }
    pub fn with_token_limit(mut self, v: usize) -> Self {
        self.token_limit = v;
        self
    }
    pub fn with_completion_reserve(mut self, v: usize) -> Self {
        self.completion_reserve = v;
        self
    }
    pub fn with_workspace(mut self, p: PathBuf) -> Self {
        self.workspace = p;
        self
    }
    pub fn with_retry(mut self, r: RetryConfig) -> Self {
        self.retry = r;
        self
    }
    pub fn with_observer(mut self, o: Arc<dyn AgentObserver>) -> Self {
        self.observer = o;
        self
    }

    pub fn build(self) -> Agent {
        let mut agent = Agent::new(
            self.llm,
            self.system_prompt,
            self.tools,
            self.max_steps,
            self.token_limit,
            self.completion_reserve,
            self.workspace,
            self.retry,
        );
        agent.set_observer(self.observer);
        agent
    }
}
