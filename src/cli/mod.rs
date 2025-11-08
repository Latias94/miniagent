use clap::{Parser, Subcommand};
use colored::*;
use std::path::PathBuf;
use std::sync::Arc;

use crate::agent::Agent;
use crate::config::Config;
use crate::llm::LlmClient;
use crate::tools::Tool;
use crate::tools::mcp::load_mcp_tools;
use crate::tools::note::{RecallNotesTool, RecordNoteTool};
use crate::tools::{
    bash::BashTool,
    file::{EditTool, ReadTool, WriteTool},
    skills::{GetSkillTool, SkillLoader},
};

mod mcp;
mod repl;
mod run;
mod skills;
mod tools;
mod userconfig;

#[derive(Parser, Debug)]
#[command(name = "miniagent", version, about = "Miniagent - Rust LLM Agent with tools & MCP", long_about = None, disable_help_subcommand = true)]
pub struct Cli {
    /// Workspace directory (default: current dir)
    #[arg(short, long)]
    pub workspace: Option<PathBuf>,

    /// Command to run (default: repl)
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Start interactive REPL (default)
    Repl,
    /// Run a single prompt and print the result
    Run { prompt: String },
    /// Tools operations
    Tools {
        #[command(subcommand)]
        cmd: tools::ToolsCmd,
    },
    /// Skills operations
    Skills {
        #[command(subcommand)]
        cmd: skills::SkillsCmd,
    },
    /// MCP operations
    Mcp {
        #[command(subcommand)]
        cmd: mcp::McpCmd,
    },
    /// Config operations
    Config {
        #[command(subcommand)]
        cmd: userconfig::ConfigCmd,
    },
}

pub async fn run_cli() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let workspace = cli.workspace.unwrap_or(std::env::current_dir()?);
    tokio::fs::create_dir_all(&workspace).await.ok();

    match cli.command.unwrap_or(Command::Repl) {
        Command::Repl => repl::repl(workspace).await,
        Command::Run { prompt } => run::run_once(workspace, prompt).await,
        Command::Tools { cmd } => tools::tools_cmd(workspace, cmd).await,
        Command::Skills { cmd } => skills::skills_cmd(workspace, cmd).await,
        Command::Mcp { cmd } => mcp::mcp_cmd(workspace, cmd).await,
        Command::Config { cmd } => userconfig::config_cmd(cmd).await,
    }
}

pub(super) async fn build_agent(
    workspace: PathBuf,
) -> anyhow::Result<(Agent, Option<Arc<tokio::sync::RwLock<SkillLoader>>>, Config)> {
    let cfg_path = Config::default_config_path();
    if !cfg_path.exists() {
        println!(
            "{}",
            "No configuration found. Creating default config from templates...".yellow()
        );
        let created = userconfig::init_user_config_noninteractive()?;
        println!("{} {}", "Created:".green(), created.display());
        println!(
            "{}",
            "Please edit the file to set your API key (api_key) and rerun.".yellow()
        );
        anyhow::bail!("configuration initialized; please set api_key")
    }

    // Load config (give helpful hint if API key is missing)
    let cfg = match Config::load_from_yaml(&cfg_path) {
        Ok(c) => c,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("Please configure a valid API Key") {
                eprintln!("{} {}", "Error:".red(), msg);
                eprintln!("{} {}", "Edit:".yellow(), cfg_path.display());
            }
            return Err(e);
        }
    };

    let llm = LlmClient::from_config(&cfg.llm).await?;

    // Tools
    let mut toolset: Vec<Arc<dyn Tool>> = Vec::new();
    if cfg.tools.enable_bash {
        toolset.push(Arc::new(BashTool {
            workspace: workspace.clone(),
        }));
    }
    if cfg.tools.enable_file_tools {
        toolset.push(Arc::new(ReadTool {
            workspace: workspace.clone(),
        }));
        toolset.push(Arc::new(WriteTool {
            workspace: workspace.clone(),
        }));
        toolset.push(Arc::new(EditTool {
            workspace: workspace.clone(),
        }));
    }
    let mut skill_loader: Option<Arc<tokio::sync::RwLock<SkillLoader>>> = None;
    if cfg.tools.enable_skills {
        let mut skills_dir = PathBuf::from(&cfg.tools.skills_dir);
        if !skills_dir.is_absolute() {
            let candidates = [
                PathBuf::from(&cfg.tools.skills_dir),
                PathBuf::from("miniagent").join(&cfg.tools.skills_dir),
                PathBuf::from("skills"),
            ];
            for c in candidates {
                if c.exists() {
                    skills_dir = c;
                    break;
                }
            }
        }
        let mut loader = SkillLoader::new(&skills_dir);
        let _ = loader.discover();
        let loader = Arc::new(tokio::sync::RwLock::new(loader));
        toolset.push(Arc::new(GetSkillTool {
            loader: loader.clone(),
        }));
        skill_loader = Some(loader);
    }
    if cfg.tools.enable_note {
        let mem = workspace.join(".agent_memory.json");
        toolset.push(Arc::new(RecordNoteTool {
            memory_file: mem.clone(),
        }));
        toolset.push(Arc::new(RecallNotesTool { memory_file: mem }));
    }
    if cfg.tools.enable_mcp {
        if let Some(mcp_path) = Config::find_config_file(&cfg.tools.mcp_config_path) {
            if let Ok(mcp_tools) = load_mcp_tools(&mcp_path).await {
                for t in mcp_tools {
                    toolset.push(t);
                }
            }
        }
    }

    // System prompt
    let system_prompt_path = Config::find_config_file(&cfg.agent.system_prompt_path)
        .unwrap_or_else(|| PathBuf::from(&cfg.agent.system_prompt_path));
    let mut system_prompt = if system_prompt_path.exists() {
        std::fs::read_to_string(&system_prompt_path)?
    } else {
        "You are miniagent, an intelligent Rust agent.".to_string()
    };

    if let Some(loader) = &skill_loader {
        let meta = loader.read().await.metadata_prompt();
        if !meta.is_empty() {
            system_prompt = system_prompt.replace("{SKILLS_METADATA}", &meta);
        } else {
            system_prompt = system_prompt.replace("{SKILLS_METADATA}", "");
        }
    } else {
        system_prompt = system_prompt.replace("{SKILLS_METADATA}", "");
    }

    if !system_prompt.contains("Current Workspace") {
        let abs = workspace.canonicalize().unwrap_or(workspace.clone());
        let appendix = format!(
            "\n\n## Current Workspace\nYou are currently working in: `{}`\nAll relative paths will be resolved relative to this directory.",
            abs.display()
        );
        system_prompt.push_str(&appendix);
    }

    let agent = Agent::builder(llm, system_prompt)
        .with_tools(toolset)
        .with_max_steps(cfg.agent.max_steps)
        .with_token_limit(cfg.agent.token_limit)
        .with_completion_reserve(cfg.agent.completion_reserve)
        .with_workspace(workspace)
        .with_retry(cfg.llm.retry.clone())
        .build();

    Ok((agent, skill_loader, cfg))
}
