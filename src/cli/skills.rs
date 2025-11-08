use super::build_agent;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum SkillsCmd {
    /// List discovered skills
    List,
    /// Show full content of a skill
    Show { name: String },
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
    }
    // avoid dropping agent without cleanup of MCP
    let _ = agent.tool_names();
    Ok(())
}
