use crate::config::Config;
use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// Initialize user config directory with examples
    Init,
}

fn copy_templates_to_user_dir() -> anyhow::Result<std::path::PathBuf> {
    let user_dir = Config::user_config_dir();
    std::fs::create_dir_all(&user_dir)?;
    // Copy example config
    let src_cfg = std::env::current_dir()?
        .join("config")
        .join("config-example.yaml");
    let dst_cfg = user_dir.join("config.yaml");
    if src_cfg.exists() {
        std::fs::copy(&src_cfg, &dst_cfg)?;
        println!("Wrote {}", dst_cfg.display());
    }
    // Copy system prompt and mcp.json
    let src_sp = std::env::current_dir()?
        .join("config")
        .join("system_prompt.md");
    let dst_sp = user_dir.join("system_prompt.md");
    if src_sp.exists() {
        std::fs::copy(&src_sp, &dst_sp)?;
        println!("Wrote {}", dst_sp.display());
    }
    let src_mcp = std::env::current_dir()?.join("config").join("mcp.json");
    let dst_mcp = user_dir.join("mcp.json");
    if src_mcp.exists() {
        std::fs::copy(&src_mcp, &dst_mcp)?;
        println!("Wrote {}", dst_mcp.display());
    }
    Ok(dst_cfg)
}

pub fn init_user_config_noninteractive() -> anyhow::Result<std::path::PathBuf> {
    copy_templates_to_user_dir()
}

pub async fn config_cmd(cmd: ConfigCmd) -> anyhow::Result<()> {
    match cmd {
        ConfigCmd::Init => {
            let _ = copy_templates_to_user_dir()?;
            println!("Done.");
        }
    }
    Ok(())
}
