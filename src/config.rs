use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub enabled: bool,
    pub max_retries: u32,
    pub initial_delay: f32,
    pub max_delay: f32,
    pub exponential_base: f32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_retries: 3,
            initial_delay: 1.0,
            max_delay: 60.0,
            exponential_base: 2.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub retry: RetryConfig,
}

fn default_provider() -> String {
    "anthropic".to_string()
}
fn default_model() -> String {
    "claude-sonnet-4-5-20250929".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_max_steps")]
    pub max_steps: usize,
    #[serde(default = "default_workspace")]
    pub workspace_dir: String,
    #[serde(default = "default_system_prompt")]
    pub system_prompt_path: String,
    #[serde(default = "default_token_limit")]
    pub token_limit: usize,
    #[serde(default = "default_completion_reserve")]
    pub completion_reserve: usize,
}

fn default_max_steps() -> usize {
    50
}
fn default_workspace() -> String {
    "./workspace".to_string()
}
fn default_system_prompt() -> String {
    "system_prompt.md".to_string()
}
fn default_token_limit() -> usize {
    80_000
}
fn default_completion_reserve() -> usize {
    2_048
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default = "default_true")]
    pub enable_file_tools: bool,
    #[serde(default = "default_true")]
    pub enable_bash: bool,
    #[serde(default = "default_true")]
    pub enable_note: bool,

    #[serde(default = "default_true")]
    pub enable_skills: bool,
    #[serde(default = "default_skills_dir")]
    pub skills_dir: String,

    #[serde(default = "default_true")]
    pub enable_mcp: bool,
    #[serde(default = "default_mcp_path")]
    pub mcp_config_path: String,
}

fn default_true() -> bool {
    true
}
fn default_skills_dir() -> String {
    "./skills".to_string()
}
fn default_mcp_path() -> String {
    "mcp.json".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub llm: LlmConfig,
    pub agent: AgentConfig,
    pub tools: ToolsConfig,
}

impl Config {
    pub fn load_from_yaml(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let raw: serde_yaml::Value = serde_yaml::from_str(&content)?;

        // Backward-compatible flat layout: if no llm section, accept flat fields
        let mut cfg = if raw.get("llm").is_none() {
            #[derive(Deserialize)]
            struct Flat {
                #[serde(default = "default_provider")]
                provider: String,
                api_key: String,
                #[serde(default = "default_model")]
                model: String,
                #[serde(default)]
                base_url: Option<String>,
                #[serde(default)]
                retry: RetryConfig,
                #[serde(default)]
                max_steps: Option<usize>,
                #[serde(default)]
                workspace_dir: Option<String>,
                #[serde(default)]
                system_prompt_path: Option<String>,
                #[serde(default)]
                completion_reserve: Option<usize>,
                #[serde(default)]
                tools: Option<ToolsConfig>,
            }
            let flat: Flat = serde_yaml::from_value(raw)?;
            Config {
                llm: LlmConfig {
                    provider: flat.provider,
                    api_key: flat.api_key,
                    model: flat.model,
                    base_url: flat.base_url,
                    retry: flat.retry,
                },
                agent: AgentConfig {
                    max_steps: flat.max_steps.unwrap_or_else(default_max_steps),
                    workspace_dir: flat.workspace_dir.unwrap_or_else(default_workspace),
                    system_prompt_path: flat
                        .system_prompt_path
                        .unwrap_or_else(default_system_prompt),
                    token_limit: default_token_limit(),
                    completion_reserve: flat
                        .completion_reserve
                        .unwrap_or_else(default_completion_reserve),
                },
                tools: flat.tools.unwrap_or(ToolsConfig {
                    enable_file_tools: true,
                    enable_bash: true,
                    enable_note: true,
                    enable_skills: true,
                    skills_dir: default_skills_dir(),
                    enable_mcp: true,
                    mcp_config_path: default_mcp_path(),
                }),
            }
        } else {
            serde_yaml::from_value(raw)?
        };

        // Apply environment variable overrides (CLI > ENV > config; we currently have no CLI overrides here)
        Self::apply_env_overrides(&mut cfg);

        // Validation: API key must be provided either via config or ENV
        if cfg.llm.api_key.is_empty() || cfg.llm.api_key == "YOUR_API_KEY_HERE" {
            anyhow::bail!(
                "Please configure a valid API Key (via config file or environment variables)"
            );
        }

        // Additional validation: generic openai-compatible requires base_url
        if cfg.llm.provider.eq_ignore_ascii_case("openai-compatible") && cfg.llm.base_url.is_none()
        {
            anyhow::bail!(
                "Provider 'openai-compatible' requires 'base_url' in config or env MINIAGENT_BASE_URL"
            );
        }
        Ok(cfg)
    }

    pub fn get_package_dir() -> PathBuf {
        // current crate dir as package dir
        std::path::PathBuf::from(env!("CARGO_PKG_NAME"))
    }

    pub fn user_config_dir() -> PathBuf {
        let mut p = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        p.push(".miniagent");
        p.push("config");
        p
    }

    pub fn find_config_file(filename: &str) -> Option<PathBuf> {
        // Priority 1: ./miniagent/config/{filename}
        let dev = std::env::current_dir()
            .ok()?
            .join("miniagent")
            .join("config")
            .join(filename);
        if dev.exists() {
            return Some(dev);
        }

        // Priority 2: ~/.miniagent/config/{filename}
        let user = Self::user_config_dir().join(filename);
        if user.exists() {
            return Some(user);
        }

        // Priority 3: ./config/{filename}
        let pkg = std::env::current_dir().ok()?.join("config").join(filename);
        if pkg.exists() {
            return Some(pkg);
        }
        None
    }

    pub fn default_config_path() -> PathBuf {
        Self::find_config_file("config.yaml").unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap()
                .join("config")
                .join("config.yaml")
        })
    }

    fn apply_env_overrides(cfg: &mut Self) {
        use std::env;

        // Provider/model/base_url overrides
        if let Ok(p) = env::var("MINIAGENT_PROVIDER") {
            cfg.llm.provider = p;
        }
        if let Ok(m) = env::var("MINIAGENT_MODEL") {
            cfg.llm.model = m;
        }
        if let Ok(u) = env::var("MINIAGENT_BASE_URL") {
            cfg.llm.base_url = Some(u);
        }

        // API key resolution
        // Priority: MINIAGENT_API_KEY > provider-specific > existing
        let provider_lc = cfg.llm.provider.to_lowercase();
        if let Ok(k) = env::var("MINIAGENT_API_KEY") {
            if !k.is_empty() {
                cfg.llm.api_key = k;
                return;
            }
        }

        // Provider-specific fallbacks
        let provider_key = match provider_lc.as_str() {
            "anthropic" => Some("ANTHROPIC_API_KEY"),
            "google" | "gemini" => Some("GEMINI_API_KEY"),
            "openai" => Some("OPENAI_API_KEY"),
            "minimax" => Some("MINIMAX_API_KEY"),
            "minimaxi" => Some("MINIMAXI_API_KEY"),
            // Generic openai-compatible: allow OPENAI_API_KEY as a convenience if present
            "openai-compatible" => Some("OPENAI_API_KEY"),
            _ => None,
        };
        if let Some(key) = provider_key {
            if let Ok(k) = env::var(key) {
                if !k.is_empty() {
                    cfg.llm.api_key = k;
                    return;
                }
            }
        }
    }
}
