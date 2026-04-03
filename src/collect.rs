use anyhow::Result;
use chrono::Utc;
use std::collections::HashSet;
use std::path::Path;

use crate::manifest::{FileEntry, FileStatus, Manifest};
use crate::utils::file_hash;

pub struct CollectResult {
    pub new_files: Vec<String>,
    pub modified_files: Vec<String>,
    pub deleted_files: Vec<String>,
    pub unchanged_files: Vec<String>,
}

pub fn collect(raw_dir: &Path, manifest: &mut Manifest) -> Result<CollectResult> {
    let mut result = CollectResult {
        new_files: vec![],
        modified_files: vec![],
        deleted_files: vec![],
        unchanged_files: vec![],
    };

    let mut seen: HashSet<String> = HashSet::new();

    if raw_dir.exists() {
        for entry in std::fs::read_dir(raw_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || path.extension().map(|e| e != "md").unwrap_or(true) {
                continue;
            }

            let filename = path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            seen.insert(filename.clone());

            let hash = file_hash(&path)?;

            if let Some(existing) = manifest.files.get_mut(&filename) {
                if existing.sha256 != hash {
                    existing.sha256 = hash;
                    existing.status = FileStatus::Pending;
                    result.modified_files.push(filename);
                } else {
                    result.unchanged_files.push(filename);
                }
            } else {
                manifest.files.insert(
                    filename.clone(),
                    FileEntry {
                        sha256: hash,
                        status: FileStatus::Pending,
                        first_seen: Utc::now(),
                        last_processed: None,
                        output_files: vec![],
                        compile_count: 0,
                    },
                );
                result.new_files.push(filename);
            }
        }
    }

    // Mark deleted files
    let all_keys: Vec<String> = manifest.files.keys().cloned().collect();
    for key in all_keys {
        if !seen.contains(&key) && manifest.files[&key].status != FileStatus::Deleted {
            manifest.files.get_mut(&key).unwrap().status = FileStatus::Deleted;
            result.deleted_files.push(key);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_vault() -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let raw = dir.path().join("raw");
        std::fs::create_dir_all(&raw).unwrap();
        (dir, raw)
    }

    #[test]
    fn first_collect_finds_new_files() {
        let (_dir, raw) = setup_vault();
        std::fs::write(raw.join("note1.md"), "# Note 1").unwrap();
        std::fs::write(raw.join("note2.md"), "# Note 2").unwrap();

        let mut manifest = Manifest::new();
        let result = collect(&raw, &mut manifest).unwrap();

        assert_eq!(result.new_files.len(), 2);
        assert!(result.modified_files.is_empty());
        assert!(result.deleted_files.is_empty());
        assert_eq!(manifest.files.len(), 2);
        for entry in manifest.files.values() {
            assert_eq!(entry.status, FileStatus::Pending);
        }
    }

    #[test]
    fn collect_detects_modification() {
        let (_dir, raw) = setup_vault();
        std::fs::write(raw.join("note.md"), "original").unwrap();

        let mut manifest = Manifest::new();
        collect(&raw, &mut manifest).unwrap();

        // Modify the file
        std::fs::write(raw.join("note.md"), "modified content").unwrap();
        let result = collect(&raw, &mut manifest).unwrap();

        assert_eq!(result.modified_files.len(), 1);
        assert!(result.new_files.is_empty());
    }

    #[test]
    fn collect_detects_deletion() {
        let (_dir, raw) = setup_vault();
        std::fs::write(raw.join("note.md"), "content").unwrap();

        let mut manifest = Manifest::new();
        collect(&raw, &mut manifest).unwrap();

        // Delete the file
        std::fs::remove_file(raw.join("note.md")).unwrap();
        let result = collect(&raw, &mut manifest).unwrap();

        assert_eq!(result.deleted_files.len(), 1);
        assert_eq!(manifest.files["note.md"].status, FileStatus::Deleted);
    }

    #[test]
    fn collect_unchanged_files() {
        let (_dir, raw) = setup_vault();
        std::fs::write(raw.join("note.md"), "content").unwrap();

        let mut manifest = Manifest::new();
        collect(&raw, &mut manifest).unwrap();

        // Re-collect without changes
        let result = collect(&raw, &mut manifest).unwrap();
        assert_eq!(result.unchanged_files.len(), 1);
        assert!(result.new_files.is_empty());
        assert!(result.modified_files.is_empty());
    }

    #[test]
    fn collect_ignores_non_md_files() {
        let (_dir, raw) = setup_vault();
        std::fs::write(raw.join("note.md"), "content").unwrap();
        std::fs::write(raw.join("image.png"), "binary").unwrap();
        std::fs::write(raw.join("data.txt"), "text").unwrap();

        let mut manifest = Manifest::new();
        let result = collect(&raw, &mut manifest).unwrap();

        assert_eq!(result.new_files.len(), 1);
        assert_eq!(manifest.files.len(), 1);
    }
}
