use anyhow::Result;
use chrono::Utc;
use crate::cli::Stage;
use crate::collect;
use crate::compile;
use crate::index;
use crate::lint;
use crate::llm::LlmBackend;
use crate::manifest::{FileStatus, Manifest};
use crate::qa;
use crate::utils::{acquire_lock, release_lock};
use crate::vault::Vault;

pub struct RunResult {
    pub collect: collect::CollectResult,
    pub compile: compile::CompileResult,
    pub lint_issues: usize,
    pub qa_reviewed: usize,
    pub indexed: bool,
}

pub async fn run(
    vault: &Vault,
    backend: &dyn LlmBackend,
    skip: &[Stage],
    jobs: usize,
    dry_run: bool,
) -> Result<RunResult> {
    let lock_path = vault.lock_path();
    acquire_lock(&lock_path)?;

    let result = run_inner(vault, backend, skip, jobs, dry_run).await;

    release_lock(&lock_path)?;

    result
}

async fn run_inner(
    vault: &Vault,
    backend: &dyn LlmBackend,
    skip: &[Stage],
    jobs: usize,
    dry_run: bool,
) -> Result<RunResult> {
    let mut manifest = vault.load_manifest()?;

    // 1. Collect
    let collect_result = collect::collect(&vault.raw_dir(), &mut manifest)?;

    // 2. Compile
    let compile_result = compile::compile(
        &mut manifest,
        &vault.raw_dir(),
        &vault.wiki_dir(),
        Some(&vault.prompts_dir()),
        backend,
        jobs,
        dry_run,
        Some(&vault.logs_dir()),
    )
    .await?;

    // 3. Lint (skippable)
    let lint_issues = if !dry_run && !skip.iter().any(|s| matches!(s, Stage::Lint)) {
        let result = lint::lint(
            &vault.wiki_dir(),
            Some(&vault.prompts_dir()),
            None, // structural lint only during pipeline run (no --deep)
            false,
            false,
        ).await?;
        result.issues.len()
    } else {
        0
    };

    // 4. QA (skippable)
    let qa_reviewed = if !dry_run && !skip.iter().any(|s| matches!(s, Stage::Qa)) {
        let result = qa::qa(
            &mut manifest,
            &vault.raw_dir(),
            &vault.wiki_dir(),
            Some(&vault.prompts_dir()),
            backend,
            false, // no auto-recompile during pipeline run
        ).await?;
        result.reviewed.len()
    } else {
        0
    };

    // 5. Index
    let indexed = if !dry_run {
        index::build_index(
            &manifest,
            &vault.wiki_dir(),
            Some(&vault.prompts_dir()),
            backend,
        )
        .await?;
        true
    } else {
        false
    };

    // Update last_run
    manifest.last_run = Some(Utc::now());
    vault.save_manifest(&manifest)?;

    Ok(RunResult {
        collect: collect_result,
        compile: compile_result,
        lint_issues,
        qa_reviewed,
        indexed,
    })
}

pub struct StatusResult {
    pub pending: usize,
    pub compiled: usize,
    pub error: usize,
    pub deleted: usize,
    pub topics: Vec<(String, u32)>,
    pub decay_scores: Option<Vec<(String, f64)>>,
}

pub fn status(manifest: &Manifest, with_decay: bool) -> StatusResult {
    let mut pending = 0;
    let mut compiled = 0;
    let mut error = 0;
    let mut deleted = 0;

    for entry in manifest.files.values() {
        match entry.status {
            FileStatus::Pending => pending += 1,
            FileStatus::Compiled => compiled += 1,
            FileStatus::Error => error += 1,
            FileStatus::Deleted => deleted += 1,
        }
    }

    let mut topics: Vec<(String, u32)> = manifest
        .topics
        .iter()
        .map(|(k, v)| (k.clone(), v.note_count))
        .collect();
    topics.sort_by(|a, b| b.1.cmp(&a.1));

    let decay_scores = if with_decay {
        let now = Utc::now();
        Some(
            manifest
                .files
                .iter()
                .filter(|(_, e)| e.status == FileStatus::Compiled)
                .filter(|(name, _)| {
                    // #evergreen 면제: raw 파일에서 태그를 확인할 수는 없지만
                    // output_files 경로의 wiki 내용을 확인
                    // 간단한 구현: 이름에 evergreen이 포함되지 않으면 decay 계산
                    !name.contains("evergreen")
                })
                .map(|(name, entry)| {
                    let days = entry
                        .last_processed
                        .map(|lp| (now - lp).num_days() as f64)
                        .unwrap_or(30.0);
                    let score = (days / 30.0).min(1.0);
                    (name.clone(), score)
                })
                .collect(),
        )
    } else {
        None
    };

    StatusResult {
        pending,
        compiled,
        error,
        deleted,
        topics,
        decay_scores,
    }
}

/// Decay scores considering #evergreen tag in wiki content
pub fn decay_with_evergreen_check(
    manifest: &Manifest,
    wiki_dir: &std::path::Path,
) -> Vec<(String, f64)> {
    let now = Utc::now();
    manifest
        .files
        .iter()
        .filter(|(_, e)| e.status == FileStatus::Compiled)
        .filter(|(_, entry)| {
            // Check if any output file contains #evergreen
            for output in &entry.output_files {
                let path = wiki_dir.join(output);
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if content.contains("#evergreen") {
                        return false; // exempt from decay
                    }
                }
            }
            true
        })
        .map(|(name, entry)| {
            let days = entry
                .last_processed
                .map(|lp| (now - lp).num_days() as f64)
                .unwrap_or(30.0);
            let score = (days / 30.0).min(1.0);
            (name.clone(), score)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{LlmBackend, LlmResponse};
    use crate::manifest::{FileEntry, FileStatus, Manifest, TopicEntry};
    use async_trait::async_trait;
    use chrono::Utc;
    use tempfile::TempDir;

    struct MockBackend;

    #[async_trait]
    impl LlmBackend for MockBackend {
        async fn call(&self, _prompt: &str, _input: &str) -> Result<LlmResponse> {
            Ok(LlmResponse {
                content: "---\ntopic: Test\n---\n# Wiki Content".to_string(),
                duration: std::time::Duration::from_millis(10),
                token_count: None,
            })
        }
        fn name(&self) -> &str {
            "mock"
        }
    }

    #[tokio::test]
    async fn e2e_init_collect_run() {
        let dir = TempDir::new().unwrap();
        let vault = Vault::new(dir.path().to_path_buf());
        vault.init().unwrap();

        // Add raw notes
        std::fs::write(
            vault.raw_dir().join("test.md"),
            "---\ntopic: Rust\n---\n# Test Note\nSome content",
        )
        .unwrap();

        let backend = MockBackend;
        let result = run(&vault, &backend, &[], 3, false).await.unwrap();

        assert_eq!(result.collect.new_files.len(), 1);
        assert_eq!(result.compile.compiled.len(), 1);
        assert!(result.indexed);

        // Verify manifest was updated
        let manifest = vault.load_manifest().unwrap();
        assert!(manifest.last_run.is_some());
        assert_eq!(manifest.files["test.md"].status, FileStatus::Compiled);
    }

    #[tokio::test]
    async fn e2e_run_with_skip_lint() {
        let dir = TempDir::new().unwrap();
        let vault = Vault::new(dir.path().to_path_buf());
        vault.init().unwrap();

        std::fs::write(
            vault.raw_dir().join("test.md"),
            "---\ntopic: Rust\n---\n# Test\nContent",
        ).unwrap();

        let backend = MockBackend;
        let result = run(&vault, &backend, &[Stage::Lint], 3, false).await.unwrap();

        assert_eq!(result.lint_issues, 0); // skipped
        assert_eq!(result.compile.compiled.len(), 1);
    }

    #[tokio::test]
    async fn e2e_run_dry_run() {
        let dir = TempDir::new().unwrap();
        let vault = Vault::new(dir.path().to_path_buf());
        vault.init().unwrap();

        std::fs::write(vault.raw_dir().join("test.md"), "# Test").unwrap();

        let backend = MockBackend;
        let result = run(&vault, &backend, &[], 3, true).await.unwrap();

        assert_eq!(result.compile.skipped, 1);
        assert!(!result.indexed);
    }

    #[test]
    fn status_counts() {
        let mut manifest = Manifest::new();
        manifest.files.insert(
            "a.md".to_string(),
            FileEntry {
                sha256: "x".to_string(),
                status: FileStatus::Pending,
                first_seen: Utc::now(),
                last_processed: None,
                output_files: vec![],
                compile_count: 0,
            },
        );
        manifest.files.insert(
            "b.md".to_string(),
            FileEntry {
                sha256: "y".to_string(),
                status: FileStatus::Compiled,
                first_seen: Utc::now(),
                last_processed: Some(Utc::now()),
                output_files: vec![],
                compile_count: 1,
            },
        );
        manifest.files.insert(
            "c.md".to_string(),
            FileEntry {
                sha256: "z".to_string(),
                status: FileStatus::Error,
                first_seen: Utc::now(),
                last_processed: None,
                output_files: vec![],
                compile_count: 0,
            },
        );
        manifest.topics.insert(
            "Rust".to_string(),
            TopicEntry {
                note_count: 5,
                last_updated: Utc::now(),
            },
        );

        let s = status(&manifest, false);
        assert_eq!(s.pending, 1);
        assert_eq!(s.compiled, 1);
        assert_eq!(s.error, 1);
        assert!(s.decay_scores.is_none());
        assert_eq!(s.topics.len(), 1);
    }

    #[test]
    fn status_with_decay() {
        let mut manifest = Manifest::new();
        manifest.files.insert(
            "recent.md".to_string(),
            FileEntry {
                sha256: "x".to_string(),
                status: FileStatus::Compiled,
                first_seen: Utc::now(),
                last_processed: Some(Utc::now()),
                output_files: vec![],
                compile_count: 1,
            },
        );

        let s = status(&manifest, true);
        let scores = s.decay_scores.unwrap();
        assert_eq!(scores.len(), 1);
        // Recently processed should have low decay
        assert!(scores[0].1 < 0.1);
    }

    #[test]
    fn decay_evergreen_exemption() {
        let dir = TempDir::new().unwrap();
        let wiki = dir.path().join("wiki").join("Topic");
        std::fs::create_dir_all(&wiki).unwrap();
        std::fs::write(
            wiki.join("evergreen_wiki.md"),
            "---\ntopic: Topic\n---\n# Evergreen\n#evergreen\nTimeless content",
        ).unwrap();
        std::fs::write(
            wiki.join("normal_wiki.md"),
            "---\ntopic: Topic\n---\n# Normal\nRegular content",
        ).unwrap();

        let mut manifest = Manifest::new();
        let old_time = Utc::now() - chrono::Duration::days(60);
        manifest.files.insert("evergreen.md".to_string(), FileEntry {
            sha256: "a".to_string(),
            status: FileStatus::Compiled,
            first_seen: old_time,
            last_processed: Some(old_time),
            output_files: vec!["Topic/evergreen_wiki.md".to_string()],
            compile_count: 1,
        });
        manifest.files.insert("normal.md".to_string(), FileEntry {
            sha256: "b".to_string(),
            status: FileStatus::Compiled,
            first_seen: old_time,
            last_processed: Some(old_time),
            output_files: vec!["Topic/normal_wiki.md".to_string()],
            compile_count: 1,
        });

        let scores = decay_with_evergreen_check(&manifest, &dir.path().join("wiki"));
        // Only normal should have decay, evergreen is exempt
        assert_eq!(scores.len(), 1);
        assert_eq!(scores[0].0, "normal.md");
        assert!(scores[0].1 > 0.5); // 60 days / 30 = 2.0, capped at 1.0
    }
}
