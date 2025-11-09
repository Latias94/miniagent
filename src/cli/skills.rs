use super::build_agent;
use anyhow::Context;
use clap::{Args, Subcommand};
use std::path::{Path, PathBuf};
use tokio::process::Command as TokioCommand;
use which::which;

#[derive(Subcommand, Debug)]
pub enum SkillsCmd {
    /// List discovered skills
    List,
    /// Show full content of a skill
    Show { name: String },
    /// Fetch (or update) Claude Skills into a local directory
    Fetch(FetchArgs),
}

#[derive(Args, Debug)]
pub struct FetchArgs {
    /// Source git repo URL
    #[arg(long, default_value = "https://github.com/anthropics/skills")]
    pub source: String,
    /// Destination directory to install skills (default: ~/.miniagent/skills)
    #[arg(long)]
    pub dest: Option<PathBuf>,
    /// Overwrite or update if destination exists
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

pub async fn skills_cmd(workspace: PathBuf, cmd: SkillsCmd) -> anyhow::Result<()> {
    let (agent, loader, _cfg) = build_agent(workspace).await?;
    match cmd {
        SkillsCmd::List => {
            if let Some(l) = loader {
                let guard = l.read().await;
                let list = guard.list();
                if list.is_empty() {
                    println!("No skills found");
                } else {
                    println!("Skills ({}):", list.len());
                    for s in list {
                        println!("  - {}", s);
                    }
                }
            } else {
                println!("Skills disabled in config");
            }
        }
        SkillsCmd::Show { name } => {
            if let Some(l) = loader {
                let guard = l.read().await;
                if let Some(s) = guard.get(&name) {
                    println!(
                        "# Skill: {}\n\n{}\n\n---\n\n{}",
                        s.name, s.description, s.content
                    );
                } else {
                    println!("Skill '{}' not found", name);
                }
            } else {
                println!("Skills disabled in config");
            }
        }
        SkillsCmd::Fetch(args) => {
            let dest = args.dest.unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".miniagent")
                    .join("skills")
            });
            fetch_or_update_skills(&args.source, &dest, args.force).await?;
            println!("Installed skills at {}", dest.display());
        }
    }
    // avoid dropping agent without cleanup of MCP
    let _ = agent.tool_names();
    Ok(())
}

pub async fn fetch_or_update_skills(source: &str, dest: &Path, force: bool) -> anyhow::Result<()> {
    let git = which("git").with_context(
        || "git executable not found on PATH; please install git or clone manually",
    )?;

    if dest.exists() {
        // If it's already a git repo, try to update
        if dest.join(".git").exists() {
            if !force {
                println!(
                    "Destination exists; updating (use --force to reclone): {}",
                    dest.display()
                );
            } else {
                println!("Force update existing repo: {}", dest.display());
            }
            let status = TokioCommand::new(&git)
                .arg("-C")
                .arg(dest)
                .arg("pull")
                .arg("--ff-only")
                .status()
                .await
                .with_context(|| "failed to run git pull")?;
            if !status.success() {
                anyhow::bail!("git pull failed (exit {})", status.code().unwrap_or(-1));
            }
            return Ok(());
        }

        if !force {
            anyhow::bail!(
                "destination exists and is not a git repo: {} (use --force to overwrite)",
                dest.display()
            );
        }
        // Remove and re-clone
        tokio::fs::remove_dir_all(dest)
            .await
            .with_context(|| format!("failed to remove {}", dest.display()))?;
    }

    tokio::fs::create_dir_all(dest.parent().unwrap_or(Path::new(".")))
        .await
        .ok();
    let status = TokioCommand::new(&git)
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(source)
        .arg(dest)
        .status()
        .await
        .with_context(|| "failed to run git clone")?;
    if !status.success() {
        anyhow::bail!("git clone failed (exit {})", status.code().unwrap_or(-1));
    }
    Ok(())
}
