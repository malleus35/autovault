use anyhow::{Context, Result};
use chrono::Utc;
use std::path::Path;
use tokio::sync::Semaphore;

use crate::llm::LlmBackend;
use crate::logging::ExecutionLog;
use crate::manifest::{FileStatus, Manifest};
use crate::parser::extract_topic;
use crate::prompts::get_prompt;
use crate::utils::atomic_write;

pub struct CompileResult {
    pub compiled: Vec<String>,
    pub merged: Vec<String>,
    pub errors: Vec<(String, String)>,
    pub skipped: usize,
}

pub async fn compile(
    manifest: &mut Manifest,
    raw_dir: &Path,
    wiki_dir: &Path,
    prompts_dir: Option<&Path>,
    backend: &dyn LlmBackend,
    jobs: usize,
    dry_run: bool,
    logs_dir: Option<&Path>,
) -> Result<CompileResult> {
    let pending: Vec<String> = manifest
        .files
        .iter()
        .filter(|(_, e)| e.status == FileStatus::Pending)
        .map(|(k, _)| k.clone())
        .collect();

    if dry_run {
        return Ok(CompileResult {
            compiled: vec![],
            merged: vec![],
            errors: vec![],
            skipped: pending.len(),
        });
    }

    let semaphore = std::sync::Arc::new(Semaphore::new(jobs));
    let mut result = CompileResult {
        compiled: vec![],
        merged: vec![],
        errors: vec![],
        skipped: 0,
    };

    let log_path = logs_dir.map(|d| d.join("run.jsonl"));

    for filename in &pending {
        let _permit = semaphore.acquire().await?;
        let start = std::time::Instant::now();
        match compile_one(filename, raw_dir, wiki_dir, prompts_dir, backend).await {
            Ok(output) => {
                let duration = start.elapsed();
                let entry = manifest.files.get_mut(filename).unwrap();
                entry.status = FileStatus::Compiled;
                entry.last_processed = Some(Utc::now());
                entry.compile_count += 1;
                entry.output_files = vec![output.wiki_path.clone()];

                if let Some(ref topic) = output.topic {
                    let topic_entry = manifest.topics.entry(topic.clone()).or_insert_with(|| {
                        crate::manifest::TopicEntry {
                            note_count: 0,
                            last_updated: Utc::now(),
                        }
                    });
                    topic_entry.note_count += 1;
                    topic_entry.last_updated = Utc::now();
                }

                if let Some(ref lp) = log_path {
                    let prompt_name = if output.was_merge { "compile_merge" } else { "compile_note" };
                    let _ = ExecutionLog {
                        timestamp: Utc::now(),
                        prompt: prompt_name.to_string(),
                        input_file: filename.clone(),
                        duration_s: duration.as_secs_f64(),
                        status: "ok".to_string(),
                        output_length: output.wiki_path.len(),
                    }.append_to_file(lp);
                }

                if output.was_merge {
                    result.merged.push(filename.clone());
                } else {
                    result.compiled.push(filename.clone());
                }
            }
            Err(e) => {
                // Retry once
                match compile_one(filename, raw_dir, wiki_dir, prompts_dir, backend).await {
                    Ok(output) => {
                        let entry = manifest.files.get_mut(filename).unwrap();
                        entry.status = FileStatus::Compiled;
                        entry.last_processed = Some(Utc::now());
                        entry.compile_count += 1;
                        entry.output_files = vec![output.wiki_path.clone()];

                        if output.was_merge {
                            result.merged.push(filename.clone());
                        } else {
                            result.compiled.push(filename.clone());
                        }
                    }
                    Err(e2) => {
                        manifest.files.get_mut(filename).unwrap().status = FileStatus::Error;
                        if let Some(ref lp) = log_path {
                            let _ = ExecutionLog {
                                timestamp: Utc::now(),
                                prompt: "compile_note".to_string(),
                                input_file: filename.clone(),
                                duration_s: start.elapsed().as_secs_f64(),
                                status: "error".to_string(),
                                output_length: 0,
                            }.append_to_file(lp);
                        }
                        result.errors.push((filename.clone(), format!("{}: {}", e, e2)));
                    }
                }
            }
        }
    }

    Ok(result)
}

struct CompileOutput {
    wiki_path: String,
    topic: Option<String>,
    was_merge: bool,
}

async fn compile_one(
    filename: &str,
    raw_dir: &Path,
    wiki_dir: &Path,
    prompts_dir: Option<&Path>,
    backend: &dyn LlmBackend,
) -> Result<CompileOutput> {
    let raw_path = raw_dir.join(filename);
    let raw_content = std::fs::read_to_string(&raw_path)
        .with_context(|| format!("reading raw file {}", filename))?;

    let topic = extract_topic(&raw_content);
    let topic_dir = topic
        .as_ref()
        .map(|t| wiki_dir.join(sanitize_dirname(t)))
        .unwrap_or_else(|| wiki_dir.join("_unsorted"));

    std::fs::create_dir_all(&topic_dir)?;

    let wiki_filename = filename.replace(".md", "_wiki.md");
    let wiki_path = topic_dir.join(&wiki_filename);

    let was_merge = wiki_path.exists();

    let (prompt_name, input) = if was_merge {
        let existing = std::fs::read_to_string(&wiki_path)?;
        let prompt = get_prompt("compile_merge", prompts_dir)
            .context("compile_merge prompt not found")?;
        (prompt, format!("## Existing Wiki\n{}\n\n## New Raw Note\n{}", existing, raw_content))
    } else {
        let prompt = get_prompt("compile_note", prompts_dir)
            .context("compile_note prompt not found")?;
        (prompt, raw_content)
    };

    let response = backend.call(&prompt_name, &input).await?;
    atomic_write(&wiki_path, response.content.as_bytes())?;

    let relative_wiki = wiki_path
        .strip_prefix(wiki_dir)
        .unwrap_or(&wiki_path)
        .to_string_lossy()
        .to_string();

    Ok(CompileOutput {
        wiki_path: relative_wiki,
        topic,
        was_merge,
    })
}

fn sanitize_dirname(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{LlmBackend, LlmResponse};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    struct MockBackend {
        call_count: AtomicUsize,
        response: String,
    }

    impl MockBackend {
        fn new(response: &str) -> Self {
            MockBackend {
                call_count: AtomicUsize::new(0),
                response: response.to_string(),
            }
        }
    }

    #[async_trait]
    impl LlmBackend for MockBackend {
        async fn call(&self, _prompt: &str, _input: &str) -> Result<LlmResponse> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(LlmResponse {
                content: self.response.clone(),
                duration: std::time::Duration::from_millis(10),
                token_count: Some(50),
            })
        }
        fn name(&self) -> &str { "mock" }
    }

    struct FailingBackend {
        fail_count: AtomicUsize,
        max_fails: usize,
    }

    impl FailingBackend {
        fn new(max_fails: usize) -> Self {
            FailingBackend {
                fail_count: AtomicUsize::new(0),
                max_fails,
            }
        }
    }

    #[async_trait]
    impl LlmBackend for FailingBackend {
        async fn call(&self, _prompt: &str, _input: &str) -> Result<LlmResponse> {
            let count = self.fail_count.fetch_add(1, Ordering::SeqCst);
            if count < self.max_fails {
                anyhow::bail!("LLM failure #{}", count + 1);
            }
            Ok(LlmResponse {
                content: "---\ntopic: Test\n---\n# Recovered".to_string(),
                duration: std::time::Duration::from_millis(10),
                token_count: None,
            })
        }
        fn name(&self) -> &str { "failing_mock" }
    }

    fn setup() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let raw = dir.path().join("raw");
        let wiki = dir.path().join("wiki");
        std::fs::create_dir_all(&raw).unwrap();
        std::fs::create_dir_all(&wiki).unwrap();
        (dir, raw, wiki)
    }

    #[tokio::test]
    async fn compile_new_note() {
        let (_dir, raw, wiki) = setup();
        std::fs::write(
            raw.join("note.md"),
            "---\ntopic: Rust\n---\n# My Note\nContent here",
        ).unwrap();

        let mut manifest = Manifest::new();
        manifest.files.insert("note.md".to_string(), crate::manifest::FileEntry {
            sha256: "abc".to_string(),
            status: FileStatus::Pending,
            first_seen: Utc::now(),
            last_processed: None,
            output_files: vec![],
            compile_count: 0,
        });

        let backend = MockBackend::new("---\ntopic: Rust\n---\n# Compiled Wiki\nProcessed content");
        let result = compile(&mut manifest, &raw, &wiki, None, &backend, 3, false, None).await.unwrap();

        assert_eq!(result.compiled.len(), 1);
        assert!(result.merged.is_empty());
        assert!(result.errors.is_empty());
        assert_eq!(manifest.files["note.md"].status, FileStatus::Compiled);
        assert_eq!(manifest.files["note.md"].compile_count, 1);
    }

    #[tokio::test]
    async fn compile_merge_existing() {
        let (_dir, raw, wiki) = setup();
        std::fs::write(
            raw.join("note.md"),
            "---\ntopic: Rust\n---\n# Updated Note",
        ).unwrap();

        // Create existing wiki
        let topic_dir = wiki.join("Rust");
        std::fs::create_dir_all(&topic_dir).unwrap();
        std::fs::write(topic_dir.join("note_wiki.md"), "# Old Wiki").unwrap();

        let mut manifest = Manifest::new();
        manifest.files.insert("note.md".to_string(), crate::manifest::FileEntry {
            sha256: "abc".to_string(),
            status: FileStatus::Pending,
            first_seen: Utc::now(),
            last_processed: None,
            output_files: vec![],
            compile_count: 0,
        });

        let backend = MockBackend::new("# Merged Wiki\nCombined content");
        let result = compile(&mut manifest, &raw, &wiki, None, &backend, 3, false, None).await.unwrap();

        assert!(result.compiled.is_empty());
        assert_eq!(result.merged.len(), 1);
    }

    #[tokio::test]
    async fn compile_llm_failure_with_retry() {
        let (_dir, raw, wiki) = setup();
        std::fs::write(raw.join("note.md"), "---\ntopic: Test\n---\n# Note").unwrap();

        let mut manifest = Manifest::new();
        manifest.files.insert("note.md".to_string(), crate::manifest::FileEntry {
            sha256: "abc".to_string(),
            status: FileStatus::Pending,
            first_seen: Utc::now(),
            last_processed: None,
            output_files: vec![],
            compile_count: 0,
        });

        // Fails once then succeeds
        let backend = FailingBackend::new(1);
        let result = compile(&mut manifest, &raw, &wiki, None, &backend, 3, false, None).await.unwrap();

        assert_eq!(result.compiled.len(), 1);
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn compile_dry_run() {
        let (_dir, raw, wiki) = setup();
        std::fs::write(raw.join("note.md"), "# Note").unwrap();

        let mut manifest = Manifest::new();
        manifest.files.insert("note.md".to_string(), crate::manifest::FileEntry {
            sha256: "abc".to_string(),
            status: FileStatus::Pending,
            first_seen: Utc::now(),
            last_processed: None,
            output_files: vec![],
            compile_count: 0,
        });

        let backend = MockBackend::new("should not be called");
        let result = compile(&mut manifest, &raw, &wiki, None, &backend, 3, true, None).await.unwrap();

        assert_eq!(result.skipped, 1);
        assert!(result.compiled.is_empty());
        // Status should remain pending
        assert_eq!(manifest.files["note.md"].status, FileStatus::Pending);
    }

    #[tokio::test]
    async fn compile_individual_file_error_isolation() {
        // When one file fails permanently, others should still compile
        let (_dir, raw, wiki) = setup();
        std::fs::write(
            raw.join("good.md"),
            "---\ntopic: Test\n---\n# Good Note",
        ).unwrap();
        // bad.md will fail because the FailingBackend fails on first 2 calls
        std::fs::write(
            raw.join("bad.md"),
            "---\ntopic: Test\n---\n# Bad Note",
        ).unwrap();

        let mut manifest = Manifest::new();
        // Insert bad.md first (alphabetical order means it gets compiled first)
        manifest.files.insert("bad.md".to_string(), crate::manifest::FileEntry {
            sha256: "x".to_string(),
            status: FileStatus::Pending,
            first_seen: Utc::now(),
            last_processed: None,
            output_files: vec![],
            compile_count: 0,
        });
        manifest.files.insert("good.md".to_string(), crate::manifest::FileEntry {
            sha256: "y".to_string(),
            status: FileStatus::Pending,
            first_seen: Utc::now(),
            last_processed: None,
            output_files: vec![],
            compile_count: 0,
        });

        // Fails first 2 calls (both attempts for bad.md), then succeeds for good.md
        let backend = FailingBackend::new(2);
        let result = compile(&mut manifest, &raw, &wiki, None, &backend, 1, false, None).await.unwrap();

        // bad.md should be Error, good.md should be Compiled
        assert_eq!(manifest.files["bad.md"].status, FileStatus::Error);
        assert_eq!(manifest.files["good.md"].status, FileStatus::Compiled);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.compiled.len(), 1);
    }
}
