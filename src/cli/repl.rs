use super::build_agent;
use crate::agent::Agent;
use crate::config::Config;
use colored::*;
use std::path::PathBuf;

pub async fn repl(workspace: PathBuf) -> anyhow::Result<()> {
    let (mut agent, _loader, cfg) = build_agent(workspace.clone()).await?;
    print_banner();
    print_session(&agent, &workspace, &cfg.llm.model);

    use rustyline::{DefaultEditor, error::ReadlineError};
    let mut rl = DefaultEditor::new()?;
    loop {
        match rl.readline("You > ") {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() {
                    continue;
                }
                if handle_builtin(&mut agent, input, &cfg).await? {
                    continue;
                }
                agent.add_user_message(input.to_string());
                println!("\n{}\n", "Agent is thinking...".dimmed());
                let _ = agent.run().await?;
                println!("\n{}\n", "-".repeat(60).dimmed());
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                println!("{}", "Exiting".yellow());
                break;
            }
            Err(e) => {
                eprintln!("Readline error: {}", e);
                break;
            }
        }
    }
    Ok(())
}

pub async fn handle_builtin(agent: &mut Agent, input: &str, cfg: &Config) -> anyhow::Result<bool> {
    if !input.starts_with('/') {
        return Ok(false);
    }
    match input.to_lowercase().as_str() {
        "/exit" | "/quit" | "/q" => {
            println!("{}", "Goodbye".yellow());
            std::process::exit(0);
        }
        "/help" => {
            print_help();
            return Ok(true);
        }
        "/clear" => {
            let sys = agent.messages.first().cloned();
            agent.messages.clear();
            if let Some(s) = sys {
                agent.messages.push(s);
            }
            println!("{}", "History cleared".green());
            return Ok(true);
        }
        "/history" => {
            println!("messages: {}", agent.messages.len());
            return Ok(true);
        }
        "/stats" => {
            let mut user = 0usize;
            let mut assistant = 0usize;
            let mut tool = 0usize;
            for m in &agent.messages {
                match m.role {
                    siumai::types::MessageRole::User => user += 1,
                    siumai::types::MessageRole::Assistant => assistant += 1,
                    siumai::types::MessageRole::Tool => tool += 1,
                    _ => {}
                }
            }
            println!(
                "messages: {} (user: {}, assistant: {}, tool: {})",
                agent.messages.len(),
                user,
                assistant,
                tool
            );
            println!("tools: {}", agent.tool_names().len());
            return Ok(true);
        }
        "/version" => {
            println!("miniagent v{}", env!("CARGO_PKG_VERSION"));
            #[cfg(feature = "tiktoken")]
            println!("feature: tiktoken");
            #[cfg(not(feature = "tiktoken"))]
            println!("feature: (no tiktoken)");
            return Ok(true);
        }
        "/config" => {
            println!("provider: {}", cfg.llm.provider);
            println!("model: {}", cfg.llm.model);
            if let Some(u) = &cfg.llm.base_url {
                println!("base_url: {}", u);
            }
            println!(
                "token_limit: {} reserve: {}",
                cfg.agent.token_limit, cfg.agent.completion_reserve
            );
            println!(
                "retry: enabled={} max_retries={} initial_delay={}s max_delay={}s base={}",
                cfg.llm.retry.enabled,
                cfg.llm.retry.max_retries,
                cfg.llm.retry.initial_delay,
                cfg.llm.retry.max_delay,
                cfg.llm.retry.exponential_base
            );
            return Ok(true);
        }
        "/tools" => {
            let names = agent.tool_names();
            if names.is_empty() {
                println!("No tools loaded");
            } else {
                println!("Loaded tools ({}):", names.len());
                for n in names {
                    println!("  - {}", n);
                }
            }
            return Ok(true);
        }
        _ => {}
    }
    Ok(false)
}

fn print_banner() {
    println!(
        "{}",
        "============================================================"
            .cyan()
            .bold()
    );
    println!(
        "{}",
        " Miniagent - Multi-turn Interactive Session ".cyan().bold()
    );
    println!(
        "{}",
        "============================================================"
            .cyan()
            .bold()
    );
}

fn print_session(agent: &Agent, workspace: &PathBuf, model: &str) {
    println!("{} {}", "Model:".dimmed(), model);
    println!("{} {}", "Workspace:".dimmed(), workspace.display());
    println!("{} {}", "Messages:".dimmed(), agent.messages.len());
}

fn print_help() {
    println!(
        "\nCommands:\n  /help     Show help\n  /clear    Clear session\n  /history  Show message count\n  /stats    Show stats\n  /tools    List loaded tools\n  /exit     Quit\n"
    );
}
