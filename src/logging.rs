use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::cli::LogLevel;

#[derive(Serialize, Deserialize)]
pub struct ExecutionLog {
    pub timestamp: DateTime<Utc>,
    pub prompt: String,
    pub input_file: String,
    pub duration_s: f64,
    pub status: String,
    pub output_length: usize,
}

impl ExecutionLog {
    pub fn append_to_file(&self, path: &Path) -> Result<()> {
        use std::fs::OpenOptions;
        use std::io::Write;

        let line = serde_json::to_string(self)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }
}

pub fn init_tracing(level: &LogLevel) {
    use tracing_subscriber::fmt;

    let filter = match level {
        LogLevel::Info => tracing::Level::INFO,
        LogLevel::Warn => tracing::Level::WARN,
        LogLevel::Error => tracing::Level::ERROR,
    };

    fmt()
        .with_max_level(filter)
        .with_target(false)
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn execution_log_append() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("run.jsonl");

        let log = ExecutionLog {
            timestamp: Utc::now(),
            prompt: "compile_note".to_string(),
            input_file: "test.md".to_string(),
            duration_s: 1.5,
            status: "ok".to_string(),
            output_length: 100,
        };
        log.append_to_file(&path).unwrap();
        log.append_to_file(&path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        let parsed: ExecutionLog = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed.prompt, "compile_note");
    }
}
