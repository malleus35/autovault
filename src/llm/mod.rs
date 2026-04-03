pub mod cli_backend;

use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;

pub struct LlmResponse {
    pub content: String,
    pub duration: Duration,
    pub token_count: Option<u64>,
}

#[async_trait]
#[allow(dead_code)]
pub trait LlmBackend: Send + Sync {
    async fn call(&self, prompt: &str, input: &str) -> Result<LlmResponse>;
    fn name(&self) -> &str;
}

pub fn detect_backend() -> Result<Box<dyn LlmBackend>> {
    // Check for claude CLI first, then fall back to others
    if which_exists("claude") {
        return Ok(Box::new(cli_backend::CliBackend::new("claude")));
    }
    anyhow::bail!("No LLM backend found. Install 'claude' CLI.")
}

fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_response_fields() {
        let r = LlmResponse {
            content: "test".to_string(),
            duration: Duration::from_secs(1),
            token_count: Some(50),
        };
        assert_eq!(r.content, "test");
        assert_eq!(r.duration.as_secs(), 1);
        assert_eq!(r.token_count, Some(50));
    }
}
