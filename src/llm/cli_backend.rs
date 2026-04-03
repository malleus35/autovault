use anyhow::{Context, Result};
use async_trait::async_trait;
use std::time::Instant;
use tokio::process::Command;

use super::{LlmBackend, LlmResponse};

pub struct CliBackend {
    command: String,
}

impl CliBackend {
    pub fn new(command: &str) -> Self {
        CliBackend {
            command: command.to_string(),
        }
    }
}

#[async_trait]
impl LlmBackend for CliBackend {
    async fn call(&self, prompt: &str, input: &str) -> Result<LlmResponse> {
        let full_input = format!("{}\n\n---\n\n{}", prompt, input);
        let start = Instant::now();

        let output = Command::new(&self.command)
            .arg("-p")
            .arg(&full_input)
            .output()
            .await
            .with_context(|| format!("running {} CLI", self.command))?;

        let duration = start.elapsed();

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("{} CLI failed: {}", self.command, stderr);
        }

        let content = String::from_utf8(output.stdout)
            .context("LLM output is not valid UTF-8")?
            .trim()
            .to_string();

        Ok(LlmResponse {
            content,
            duration,
            token_count: None,
        })
    }

    fn name(&self) -> &str {
        &self.command
    }
}
