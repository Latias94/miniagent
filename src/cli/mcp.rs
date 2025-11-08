use crate::config::Config;
use crate::tools::mcp::{cleanup_mcp, load_mcp_tools};
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum McpCmd {
    /// List MCP tools from config
    List,
}

pub async fn mcp_cmd(_workspace: PathBuf, cmd: McpCmd) -> anyhow::Result<()> {
    match cmd {
        McpCmd::List => {
            let cfg_path = Config::default_config_path();
            let cfg = Config::load_from_yaml(&cfg_path)?;
            if let Some(mcp_path) = Config::find_config_file(&cfg.tools.mcp_config_path) {
                match load_mcp_tools(&mcp_path).await {
                    Ok(tools) => {
                        if tools.is_empty() {
                            println!("No MCP tools found");
                        } else {
                            println!("MCP tools ({}):", tools.len());
                            for t in tools {
                                println!("  - {}", t.name());
                            }
                        }
                    }
                    Err(e) => println!("Failed to load MCP tools: {}", e),
                }
                cleanup_mcp().await;
            } else {
                println!("MCP config not found: {}", cfg.tools.mcp_config_path);
            }
        }
    }
    Ok(())
}
