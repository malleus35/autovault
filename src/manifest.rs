use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::utils::atomic_write;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Manifest {
    pub version: String,
    pub last_run: Option<DateTime<Utc>>,
    pub files: HashMap<String, FileEntry>,
    pub topics: HashMap<String, TopicEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileEntry {
    pub sha256: String,
    pub status: FileStatus,
    pub first_seen: DateTime<Utc>,
    pub last_processed: Option<DateTime<Utc>>,
    pub output_files: Vec<String>,
    pub compile_count: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FileStatus {
    Pending,
    Compiled,
    Error,
    Deleted,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TopicEntry {
    pub note_count: u32,
    pub last_updated: DateTime<Utc>,
}

impl Manifest {
    pub fn new() -> Self {
        Manifest {
            version: "1.0".to_string(),
            last_run: None,
            files: HashMap::new(),
            topics: HashMap::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let data = std::fs::read_to_string(path)
            .with_context(|| format!("reading manifest {}", path.display()))?;
        let manifest: Manifest =
            serde_json::from_str(&data).context("parsing manifest JSON")?;
        Ok(manifest)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self).context("serializing manifest")?;
        atomic_write(path, json.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn new_manifest_has_defaults() {
        let m = Manifest::new();
        assert_eq!(m.version, "1.0");
        assert!(m.last_run.is_none());
        assert!(m.files.is_empty());
        assert!(m.topics.is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("manifest.json");

        let mut m = Manifest::new();
        m.files.insert(
            "test.md".to_string(),
            FileEntry {
                sha256: "abc123".to_string(),
                status: FileStatus::Pending,
                first_seen: Utc::now(),
                last_processed: None,
                output_files: vec![],
                compile_count: 0,
            },
        );
        m.save(&path).unwrap();

        let loaded = Manifest::load(&path).unwrap();
        assert_eq!(loaded.files.len(), 1);
        assert_eq!(loaded.files["test.md"].sha256, "abc123");
        assert_eq!(loaded.files["test.md"].status, FileStatus::Pending);
    }

    #[test]
    fn file_status_serde_lowercase() {
        let entry = FileEntry {
            sha256: "x".to_string(),
            status: FileStatus::Compiled,
            first_seen: Utc::now(),
            last_processed: None,
            output_files: vec![],
            compile_count: 1,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"compiled\""));
    }
}
