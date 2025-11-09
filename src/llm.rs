use crate::config::{LlmConfig, RetryConfig};
use anyhow::Result;
use siumai::retry_api::{RetryBackend, RetryOptions, RetryPolicy};

#[derive(Clone)]
pub struct LlmClient {
    inner: siumai::provider::Siumai,
}

impl LlmClient {
    pub async fn from_config(cfg: &LlmConfig) -> Result<Self> {
        // Normalize provider id: collapse aliases and generic openai-compatible
        let provider_norm = match cfg.provider.to_lowercase().as_str() {
            // Aliases
            "google" => "gemini",
            "minimax" => "minimaxi",
            // Generic OpenAI-compatible routes through OpenAI with custom base_url
            "openai-compatible" => "openai",
            other => other,
        };

        // Single path: provider_id + optional base_url + retry
        let mut b = siumai::provider::Siumai::builder()
            .provider_id(provider_norm)
            .api_key(cfg.api_key.clone())
            .model(cfg.model.clone());
        if let Some(url) = &cfg.base_url {
            b = b.base_url(url.clone());
        }
        if let Some(options) = to_retry_options(&cfg.retry) {
            b = b.with_retry(options);
        }
        let client = match b.build().await {
            Ok(c) => c,
            Err(e) => {
                // Fallback: prefer generic OpenAI-compatible path if a base_url is provided
                if let Some(url) = &cfg.base_url {
                    let mut fb = siumai::provider::Siumai::builder()
                        .openai()
                        .api_key(cfg.api_key.clone())
                        .model(cfg.model.clone())
                        .base_url(url.clone());
                    if let Some(options) = to_retry_options(&cfg.retry) {
                        fb = fb.with_retry(options);
                    }
                    tracing::warn!(
                        "Falling back to openai-compatible (openai + base_url) due to provider build error: {}",
                        e
                    );
                    fb.build().await?
                } else {
                    return Err(anyhow::anyhow!(
                        "Failed to build provider '{}': {}. To use fallback (openai-compatible), please provide 'base_url' in config.",
                        provider_norm,
                        e
                    ));
                }
            }
        };
        Ok(Self { inner: client })
    }

    pub fn inner(&self) -> &siumai::provider::Siumai {
        &self.inner
    }
}

fn to_retry_options(cfg: &RetryConfig) -> Option<RetryOptions> {
    if !cfg.enabled {
        return None;
    }
    let policy = RetryPolicy::new()
        .with_max_attempts(cfg.max_retries)
        .with_initial_delay(std::time::Duration::from_secs_f32(cfg.initial_delay))
        .with_max_delay(std::time::Duration::from_secs_f32(cfg.max_delay))
        .with_backoff_multiplier(cfg.exponential_base as f64)
        .with_jitter(true);
    Some(RetryOptions {
        backend: RetryBackend::Policy,
        provider: None,
        policy: Some(policy),
        retry_401: true,
        idempotent: true,
    })
}
