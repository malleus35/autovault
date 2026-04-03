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
    // Try to resolve the actual path of the claude CLI
    if let Some(path) = resolve_command("claude") {
        return Ok(Box::new(cli_backend::CliBackend::new(&path)));
    }
    anyhow::bail!("No LLM backend found. Install 'claude' CLI.")
}

fn resolve_command(cmd: &str) -> Option<String> {
    // Check common known paths first
    let known_paths = [
        format!("{}/.claude/local/{}", std::env::var("HOME").unwrap_or_default(), cmd),
        format!("/usr/local/bin/{}", cmd),
        format!("/opt/homebrew/bin/{}", cmd),
    ];
    for path in &known_paths {
        if std::path::Path::new(path).exists() {
            return Some(path.clone());
        }
    }
    // Fall back to `which`
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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
