use super::build_agent;
use std::path::PathBuf;

pub async fn run_once(workspace: PathBuf, prompt: String) -> anyhow::Result<()> {
    let (mut agent, _loader, _cfg) = build_agent(workspace).await?;
    agent.add_user_message(prompt);
    let output = agent.run().await?;
    if !output.is_empty() {
        println!("{}", output);
    }
    Ok(())
}
