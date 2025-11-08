use super::build_agent;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum ToolsCmd {
    /// List loaded tools
    List,
    /// Describe a tool (schema)
    Describe { name: String },
    /// Call a tool with JSON args
    Call {
        name: String,
        #[arg(short, long)]
        args: Option<String>,
    },
}

pub async fn tools_cmd(workspace: PathBuf, cmd: ToolsCmd) -> anyhow::Result<()> {
    let (agent, _loader, _cfg) = build_agent(workspace).await?;
    match cmd {
        ToolsCmd::List => {
            let names = agent.tool_names();
            if names.is_empty() {
                println!("No tools loaded");
            } else {
                println!("Loaded tools ({}):", names.len());
                for n in names {
                    println!("  - {}", n);
                }
            }
        }
        ToolsCmd::Describe { name } => match agent.tool_schema(&name) {
            Some((n, desc, params)) => {
                println!(
                    "name: {}\ndescription: {}\nparameters:\n{}",
                    n,
                    desc,
                    serde_json::to_string_pretty(&params).unwrap_or_default()
                );
            }
            None => println!("Tool '{}' not found", name),
        },
        ToolsCmd::Call { name, args } => {
            let parsed: serde_json::Value = match args {
                Some(s) => match serde_json::from_str(&s) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("Invalid JSON for --args: {}", e);
                        serde_json::json!({})
                    }
                },
                None => serde_json::json!({}),
            };
            match agent.call_tool_direct(&name, parsed).await {
                Some(res) => {
                    println!("success: {}", res.success);
                    if res.success {
                        println!("content:\n{}", res.content);
                    }
                    if let Some(err) = res.error {
                        println!("error:\n{}", err);
                    }
                }
                None => println!("Tool '{}' not found", name),
            }
        }
    }
    Ok(())
}
