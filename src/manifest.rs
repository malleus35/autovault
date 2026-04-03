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

    #[test]
    fn json_compatibility_with_bash_format() {
        // Simulate a manifest.json from the bash version
        let json_str = r#"{
            "version": "1.0",
            "last_run": "2025-01-15T10:30:00Z",
            "files": {
                "meeting-notes.md": {
                    "sha256": "a1b2c3d4e5f6",
                    "status": "compiled",
                    "first_seen": "2025-01-10T08:00:00Z",
                    "last_processed": "2025-01-15T10:30:00Z",
                    "output_files": ["Work/meeting-notes_wiki.md"],
                    "compile_count": 2
                },
                "new-idea.md": {
                    "sha256": "f6e5d4c3b2a1",
                    "status": "pending",
                    "first_seen": "2025-01-14T12:00:00Z",
                    "last_processed": null,
                    "output_files": [],
                    "compile_count": 0
                }
            },
            "topics": {
                "Work": {
                    "note_count": 5,
                    "last_updated": "2025-01-15T10:30:00Z"
                }
            }
        }"#;

        let manifest: Manifest = serde_json::from_str(json_str).unwrap();
        assert_eq!(manifest.version, "1.0");
        assert!(manifest.last_run.is_some());
        assert_eq!(manifest.files.len(), 2);
        assert_eq!(manifest.files["meeting-notes.md"].status, FileStatus::Compiled);
        assert_eq!(manifest.files["meeting-notes.md"].compile_count, 2);
        assert_eq!(manifest.files["new-idea.md"].status, FileStatus::Pending);
        assert!(manifest.files["new-idea.md"].last_processed.is_none());
        assert_eq!(manifest.topics["Work"].note_count, 5);

        // Re-serialize and verify it roundtrips
        let reserialized = serde_json::to_string_pretty(&manifest).unwrap();
        let reloaded: Manifest = serde_json::from_str(&reserialized).unwrap();
        assert_eq!(reloaded.files.len(), 2);
        assert_eq!(reloaded.topics.len(), 1);
    }

    #[test]
    fn all_file_statuses_roundtrip() {
        for (status, expected_str) in [
            (FileStatus::Pending, "pending"),
            (FileStatus::Compiled, "compiled"),
            (FileStatus::Error, "error"),
            (FileStatus::Deleted, "deleted"),
        ] {
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(json, format!("\"{}\"", expected_str));
            let deserialized: FileStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, status);
        }
    }
}
