use anyhow::{Context, Result};
use std::path::Path;

use crate::llm::LlmBackend;
use crate::manifest::{FileStatus, Manifest};
use crate::prompts::get_prompt;

pub struct QaResult {
    pub reviewed: Vec<QaReview>,
    pub recompile_triggered: Vec<String>,
}

pub struct QaReview {
    pub raw_file: String,
    pub wiki_file: String,
    pub score: u32,
    pub feedback: String,
}

pub async fn qa(
    manifest: &mut Manifest,
    raw_dir: &Path,
    wiki_dir: &Path,
    prompts_dir: Option<&Path>,
    backend: &dyn LlmBackend,
    recompile: bool,
) -> Result<QaResult> {
    let prompt = get_prompt("qa_review", prompts_dir)
        .context("qa_review prompt not found")?;

    let mut result = QaResult {
        reviewed: vec![],
        recompile_triggered: vec![],
    };

    let compiled: Vec<(String, Vec<String>)> = manifest
        .files
        .iter()
        .filter(|(_, e)| e.status == FileStatus::Compiled)
        .map(|(k, e)| (k.clone(), e.output_files.clone()))
        .collect();

    for (raw_name, output_files) in &compiled {
        let raw_path = raw_dir.join(raw_name);
        if !raw_path.exists() {
            continue;
        }
        let raw_content = std::fs::read_to_string(&raw_path)?;

        for wiki_rel in output_files {
            let wiki_path = wiki_dir.join(wiki_rel);
            if !wiki_path.exists() {
                continue;
            }
            let wiki_content = std::fs::read_to_string(&wiki_path)?;

            let input = format!(
                "## Raw Note ({})\n{}\n\n## Wiki Note ({})\n{}",
                raw_name, raw_content, wiki_rel, wiki_content
            );

            match backend.call(&prompt, &input).await {
                Ok(response) => {
                    let (score, feedback) = parse_qa_response(&response.content);
                    result.reviewed.push(QaReview {
                        raw_file: raw_name.clone(),
                        wiki_file: wiki_rel.clone(),
                        score,
                        feedback,
                    });

                    if recompile && score < 3 {
                        manifest.files.get_mut(raw_name).unwrap().status = FileStatus::Pending;
                        result.recompile_triggered.push(raw_name.clone());
                    }
                }
                Err(_) => {
                    // Skip QA errors for individual files
                }
            }
        }
    }

    Ok(result)
}

fn parse_qa_response(content: &str) -> (u32, String) {
    if let Ok(json) = crate::parser::parse_json_response(content) {
        let score = json
            .get("score")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let feedback = json
            .get("feedback")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        (score, feedback)
    } else {
        (0, content.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{LlmBackend, LlmResponse};
    use crate::manifest::{FileEntry, FileStatus, Manifest};
    use async_trait::async_trait;
    use chrono::Utc;
    use tempfile::TempDir;

    struct MockQaBackend {
        score: u32,
    }

    #[async_trait]
    impl LlmBackend for MockQaBackend {
        async fn call(&self, _prompt: &str, _input: &str) -> Result<LlmResponse> {
            Ok(LlmResponse {
                content: format!(
                    r#"{{"score": {}, "feedback": "Test feedback"}}"#,
                    self.score
                ),
                duration: std::time::Duration::from_millis(10),
                token_count: None,
            })
        }
        fn name(&self) -> &str { "mock_qa" }
    }

    fn setup() -> (TempDir, std::path::PathBuf, std::path::PathBuf, Manifest) {
        let dir = TempDir::new().unwrap();
        let raw = dir.path().join("raw");
        let wiki_topic = dir.path().join("wiki").join("Topic");
        let wiki = dir.path().join("wiki");
        std::fs::create_dir_all(&raw).unwrap();
        std::fs::create_dir_all(&wiki_topic).unwrap();

        std::fs::write(raw.join("note.md"), "# Raw content").unwrap();
        std::fs::write(wiki_topic.join("note_wiki.md"), "# Wiki content").unwrap();

        let mut manifest = Manifest::new();
        manifest.files.insert("note.md".to_string(), FileEntry {
            sha256: "abc".to_string(),
            status: FileStatus::Compiled,
            first_seen: Utc::now(),
            last_processed: Some(Utc::now()),
            output_files: vec!["Topic/note_wiki.md".to_string()],
            compile_count: 1,
        });

        (dir, raw, wiki, manifest)
    }

    #[tokio::test]
    async fn qa_normal_review() {
        let (_dir, raw, wiki, mut manifest) = setup();
        let backend = MockQaBackend { score: 4 };

        let result = qa(&mut manifest, &raw, &wiki, None, &backend, false).await.unwrap();

        assert_eq!(result.reviewed.len(), 1);
        assert_eq!(result.reviewed[0].score, 4);
        assert_eq!(result.reviewed[0].feedback, "Test feedback");
        assert!(result.recompile_triggered.is_empty());
    }

    #[tokio::test]
    async fn qa_low_score_detected() {
        let (_dir, raw, wiki, mut manifest) = setup();
        let backend = MockQaBackend { score: 2 };

        let result = qa(&mut manifest, &raw, &wiki, None, &backend, false).await.unwrap();

        assert_eq!(result.reviewed.len(), 1);
        assert_eq!(result.reviewed[0].score, 2);
        // Without recompile flag, status should remain
        assert_eq!(manifest.files["note.md"].status, FileStatus::Compiled);
    }

    #[tokio::test]
    async fn qa_recompile_trigger() {
        let (_dir, raw, wiki, mut manifest) = setup();
        let backend = MockQaBackend { score: 2 };

        let result = qa(&mut manifest, &raw, &wiki, None, &backend, true).await.unwrap();

        assert_eq!(result.recompile_triggered.len(), 1);
        assert_eq!(manifest.files["note.md"].status, FileStatus::Pending);
    }
}
