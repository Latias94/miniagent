use crate::config::LlmConfig;
use anyhow::Result;

#[derive(Clone)]
pub struct LlmClient {
    inner: siumai::provider::Siumai,
}

impl LlmClient {
    pub async fn from_config(cfg: &LlmConfig) -> Result<Self> {
        // Normalize provider id
        let provider = cfg.provider.to_lowercase();

        // Always build the unified Siumai client across all branches
        let client = if provider == "anthropic" {
            let mut b = siumai::provider::Siumai::builder()
                .anthropic()
                .api_key(cfg.api_key.clone())
                .model(cfg.model.clone());
            if let Some(url) = &cfg.base_url {
                b = b.base_url(url.clone());
            }
            b.build().await?
        } else if provider == "openai" {
            let mut b = siumai::provider::Siumai::builder()
                .openai()
                .api_key(cfg.api_key.clone())
                .model(cfg.model.clone());
            if let Some(url) = &cfg.base_url {
                b = b.base_url(url.clone());
            }
            b.build().await?
        } else if provider == "minimaxi" || provider == "minimax" {
            // MiniMaxi: native provider (recommended)
            let mut b = siumai::provider::Siumai::builder()
                .minimaxi()
                .api_key(cfg.api_key.clone())
                .model(cfg.model.clone());
            if let Some(url) = &cfg.base_url {
                b = b.base_url(url.clone());
            }
            b.build().await?
        } else if provider == "openai-compatible" {
            // Generic OpenAI-compatible: route through OpenAI with required base_url
            let mut b = siumai::provider::Siumai::builder()
                .openai()
                .api_key(cfg.api_key.clone())
                .model(cfg.model.clone());
            if let Some(url) = &cfg.base_url {
                b = b.base_url(url.clone());
            }
            b.build().await?
        } else {
            // Fallback: try provider_id directly (openai-compatible adapters), else Anthropic
            let mut b = siumai::provider::Siumai::builder()
                .provider_id(provider.clone())
                .api_key(cfg.api_key.clone())
                .model(cfg.model.clone());
            if let Some(url) = &cfg.base_url {
                b = b.base_url(url.clone());
            }
            match b.build().await {
                Ok(c) => c,
                Err(_) => {
                    // Fallback to Anthropic if unknown provider id mapping fails
                    let mut b = siumai::provider::Siumai::builder()
                        .anthropic()
                        .api_key(cfg.api_key.clone())
                        .model(cfg.model.clone());
                    if let Some(url) = &cfg.base_url {
                        b = b.base_url(url.clone());
                    }
                    b.build().await?
                }
            }
        };
        Ok(Self { inner: client })
    }

    pub fn inner(&self) -> &siumai::provider::Siumai {
        &self.inner
    }
}
