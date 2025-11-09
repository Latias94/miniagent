use clap::{Parser, Subcommand};
use colored::*;
use std::path::PathBuf;
use std::sync::Arc;

use crate::agent::Agent;
use crate::cli::skills::fetch_or_update_skills;
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
#[cfg(feature = "embed-skills")]
use include_dir::{Dir, include_dir};
#[cfg(target_os = "windows")]
use which::which;

mod mcp;
mod repl;
mod run;
mod skills;
mod tools;
mod userconfig;

// Embed the entire skills directory into the binary
#[cfg(feature = "embed-skills")]
static EMBEDDED_SKILLS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/skills");

#[derive(Parser, Debug)]
#[command(
    name = "miniagent",
    version,
    about = "Miniagent - Rust LLM Agent with tools & MCP",
    long_about = None,
    disable_help_subcommand = true,
    // Ensure `miniagent skills list` is parsed as subcommand, not workspace path
    subcommand_precedence_over_arg = true,
)]
pub struct Cli {
    /// Workspace directory (default: current dir)
    #[arg(short, long)]
    pub workspace: Option<PathBuf>,
    /// Optional positional workspace directory, e.g. `miniagent ./my-workspace`
    /// Conflicts with --workspace
    #[arg(value_name = "WORKSPACE", conflicts_with = "workspace")]
    pub workspace_pos: Option<PathBuf>,

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
    let workspace = cli
        .workspace
        .or(cli.workspace_pos)
        .unwrap_or(std::env::current_dir()?);
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
        // If skills are not embedded, attempt to fetch them on first run to improve UX
        #[cfg(not(feature = "embed-skills"))]
        {
            if let Some(home) = dirs::home_dir() {
                let target = home.join(".miniagent").join("skills");
                println!(
                    "{} {}",
                    "No skills found; attempting to fetch into".yellow(),
                    target.display()
                );
                if let Err(e) =
                    fetch_or_update_skills("https://github.com/anthropics/skills", &target, false)
                        .await
                {
                    eprintln!("{} {}", "Auto-fetch of Claude Skills failed:".yellow(), e);
                    eprintln!(
                        "{}",
                        "Tip: run 'miniagent skills fetch' later to install skills."
                    );
                } else {
                    println!(
                        "{} {}",
                        "Installed Claude Skills to".green(),
                        target.display()
                    );
                }
            }
        }
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

    let llm_primary = LlmClient::from_config(&cfg.llm).await?;

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
            // Search priority:
            // 1) Path from config (relative to CWD)
            // 2) ./miniagent/<skills_dir>
            // 3) ./skills
            // 4) ~/.miniagent/skills (user-shared skills location)
            let mut candidates = Vec::new();
            candidates.push(PathBuf::from(&cfg.tools.skills_dir));
            candidates.push(PathBuf::from("miniagent").join(&cfg.tools.skills_dir));
            candidates.push(PathBuf::from("skills"));
            if let Some(home) = dirs::home_dir() {
                candidates.push(home.join(".miniagent").join("skills"));
            }
            let mut found = false;
            for c in candidates {
                if c.exists() {
                    skills_dir = c;
                    found = true;
                    break;
                }
            }
            if !found {
                // No on-disk skills found; try to extract embedded skills (if enabled),
                // otherwise fetch them automatically into ~/.miniagent/skills.
                #[cfg(feature = "embed-skills")]
                {
                    if let Some(home) = dirs::home_dir() {
                        let target = home.join(".miniagent").join("skills");
                        if !target.exists() {
                            if let Err(e) = EMBEDDED_SKILLS.extract(&target) {
                                eprintln!(
                                    "{} {}",
                                    "Failed to extract embedded skills:".yellow(),
                                    e
                                );
                            } else {
                                println!(
                                    "{} {}",
                                    "Installed embedded skills to".green(),
                                    target.display()
                                );
                            }
                        }
                        if target.exists() {
                            skills_dir = target;
                        }
                    }
                }
                #[cfg(not(feature = "embed-skills"))]
                {
                    if let Some(home) = dirs::home_dir() {
                        let target = home.join(".miniagent").join("skills");
                        if !target.exists() {
                            if let Err(e) = fetch_or_update_skills(
                                "https://github.com/anthropics/skills",
                                &target,
                                false,
                            )
                            .await
                            {
                                eprintln!(
                                    "{} {}",
                                    "Failed to auto-fetch Claude Skills:".yellow(),
                                    e
                                );
                                eprintln!(
                                    "{}",
                                    "You can also run 'miniagent skills fetch' manually."
                                );
                            }
                        }
                        if target.exists() {
                            println!(
                                "{} {}",
                                "Installed Claude Skills to".green(),
                                target.display()
                            );
                            skills_dir = target;
                        }
                    }
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

    // Append execution environment details to help the model choose correct commands
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    #[cfg(target_os = "windows")]
    let default_shell: String = {
        if which("pwsh").is_ok() {
            "pwsh -NoLogo -Command".to_string()
        } else if which("powershell").is_ok() {
            "powershell -NoLogo -Command".to_string()
        } else {
            "cmd.exe /C".to_string()
        }
    };
    #[cfg(not(target_os = "windows"))]
    let default_shell: String = "bash -lc".to_string();
    let path_sep = if cfg!(target_os = "windows") {
        "\\"
    } else {
        "/"
    };
    let env_section = format!(
        "\n\n## Execution Environment\n- OS: {} ({})\n- Default shell for tool 'bash': {}\n- Path separator: {}\n- Tip: On Windows, prefer PowerShell-friendly commands (e.g., Get-ChildItem -Force instead of 'ls -la').",
        os, arch, default_shell, path_sep
    );
    if !system_prompt.contains("## Execution Environment") {
        system_prompt.push_str(&env_section);
    }

    if !system_prompt.contains("Current Workspace") {
        let abs = workspace.canonicalize().unwrap_or(workspace.clone());
        let appendix = format!(
            "\n\n## Current Workspace\nYou are currently working in: `{}`\nAll relative paths will be resolved relative to this directory.",
            abs.display()
        );
        system_prompt.push_str(&appendix);
    }

    let agent = Agent::builder(llm_primary.clone(), system_prompt)
        .with_tools(toolset)
        .with_max_steps(cfg.agent.max_steps)
        .with_token_limit(cfg.agent.token_limit)
        .with_completion_reserve(cfg.agent.completion_reserve)
        .with_workspace(workspace)
        .with_retry(cfg.llm.retry.clone())
        .build();

    Ok((agent, skill_loader, cfg))
}
