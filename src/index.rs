use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

use crate::llm::LlmBackend;
use crate::manifest::Manifest;
use crate::prompts::get_prompt;
use crate::utils::atomic_write;

const TEMPLATE_THRESHOLD: usize = 5;

pub async fn build_index(
    manifest: &Manifest,
    wiki_dir: &Path,
    prompts_dir: Option<&Path>,
    backend: &dyn LlmBackend,
) -> Result<()> {
    // Group wiki files by topic
    let mut topic_files: HashMap<String, Vec<String>> = HashMap::new();
    for (_filename, entry) in &manifest.files {
        for output in &entry.output_files {
            // output is like "TopicName/note_wiki.md"
            if let Some(topic_name) = output.split('/').next() {
                topic_files
                    .entry(topic_name.to_string())
                    .or_default()
                    .push(output.clone());
            }
        }
    }

    // Also gather from manifest.topics
    for topic_name in manifest.topics.keys() {
        let sanitized = sanitize_dirname(topic_name);
        let topic_dir = wiki_dir.join(&sanitized);
        if topic_dir.exists() {
            let files: Vec<String> = std::fs::read_dir(&topic_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path().is_file()
                        && e.path().extension().map(|ext| ext == "md").unwrap_or(false)
                        && e.file_name().to_string_lossy() != "_index.md"
                })
                .map(|e| format!("{}/{}", sanitized, e.file_name().to_string_lossy()))
                .collect();

            if !files.is_empty() {
                topic_files.insert(sanitized.clone(), files);
            }
        }
    }

    // Build per-topic index
    let mut topic_summaries: Vec<(String, usize)> = Vec::new();
    for (topic_name, files) in &topic_files {
        let index_content = build_topic_index(topic_name, files, wiki_dir, prompts_dir, backend).await?;
        let index_path = wiki_dir.join(topic_name).join("_index.md");
        atomic_write(&index_path, index_content.as_bytes())?;
        topic_summaries.push((topic_name.clone(), files.len()));
    }

    // Build top-level index
    let top_content = build_top_index(&topic_summaries, wiki_dir, prompts_dir, backend).await?;
    let top_path = wiki_dir.join("_index.md");
    atomic_write(&top_path, top_content.as_bytes())?;

    Ok(())
}

async fn build_topic_index(
    topic_name: &str,
    files: &[String],
    wiki_dir: &Path,
    prompts_dir: Option<&Path>,
    backend: &dyn LlmBackend,
) -> Result<String> {
    if files.len() <= TEMPLATE_THRESHOLD {
        // Simple template
        let mut content = format!("# {}\n\n", topic_name);
        for file in files {
            let note_name = file
                .rsplit('/')
                .next()
                .unwrap_or(file)
                .replace("_wiki.md", "")
                .replace(".md", "");
            content.push_str(&format!("- [[{}]]\n", note_name));
        }
        Ok(content)
    } else {
        // Use LLM for richer index
        let prompt = get_prompt("index_topic", prompts_dir)
            .context("index_topic prompt not found")?;

        let mut file_list = String::new();
        for file in files {
            let path = wiki_dir.join(file);
            if let Ok(content) = std::fs::read_to_string(&path) {
                let preview: String = content.lines().take(5).collect::<Vec<_>>().join("\n");
                file_list.push_str(&format!("## {}\n{}\n\n", file, preview));
            }
        }

        let input = format!("Topic: {}\nFiles: {}\n\n{}", topic_name, files.len(), file_list);
        let response = backend.call(&prompt, &input).await?;
        Ok(response.content)
    }
}

async fn build_top_index(
    topics: &[(String, usize)],
    _wiki_dir: &Path,
    prompts_dir: Option<&Path>,
    backend: &dyn LlmBackend,
) -> Result<String> {
    let total_notes: usize = topics.iter().map(|(_, n)| n).sum();

    if total_notes <= TEMPLATE_THRESHOLD {
        let mut content = "# Knowledge Base Index\n\n".to_string();
        for (topic, count) in topics {
            content.push_str(&format!("- [[{}/_index|{}]] ({} notes)\n", topic, topic, count));
        }
        Ok(content)
    } else {
        let prompt = get_prompt("index_top", prompts_dir)
            .context("index_top prompt not found")?;

        let mut topic_list = String::new();
        for (topic, count) in topics {
            topic_list.push_str(&format!("- {} ({} notes)\n", topic, count));
        }

        let response = backend.call(&prompt, &topic_list).await?;
        Ok(response.content)
    }
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
    use crate::manifest::{FileEntry, FileStatus, TopicEntry};
    use async_trait::async_trait;
    use chrono::Utc;
    use tempfile::TempDir;

    struct MockBackend;

    #[async_trait]
    impl LlmBackend for MockBackend {
        async fn call(&self, _prompt: &str, input: &str) -> Result<LlmResponse> {
            Ok(LlmResponse {
                content: format!("# LLM Generated Index\n\n{}", input),
                duration: std::time::Duration::from_millis(10),
                token_count: None,
            })
        }
        fn name(&self) -> &str { "mock" }
    }

    fn create_test_vault() -> (TempDir, Manifest) {
        let dir = TempDir::new().unwrap();
        let wiki = dir.path().join("wiki");
        let topic_dir = wiki.join("Rust");
        std::fs::create_dir_all(&topic_dir).unwrap();

        // Create 2 wiki files
        std::fs::write(topic_dir.join("note1_wiki.md"), "# Note 1\nRust basics").unwrap();
        std::fs::write(topic_dir.join("note2_wiki.md"), "# Note 2\nRust advanced").unwrap();

        let mut manifest = Manifest::new();
        manifest.files.insert("note1.md".to_string(), FileEntry {
            sha256: "a".to_string(),
            status: FileStatus::Compiled,
            first_seen: Utc::now(),
            last_processed: Some(Utc::now()),
            output_files: vec!["Rust/note1_wiki.md".to_string()],
            compile_count: 1,
        });
        manifest.files.insert("note2.md".to_string(), FileEntry {
            sha256: "b".to_string(),
            status: FileStatus::Compiled,
            first_seen: Utc::now(),
            last_processed: Some(Utc::now()),
            output_files: vec!["Rust/note2_wiki.md".to_string()],
            compile_count: 1,
        });
        manifest.topics.insert("Rust".to_string(), TopicEntry {
            note_count: 2,
            last_updated: Utc::now(),
        });

        (dir, manifest)
    }

    #[tokio::test]
    async fn build_index_creates_files() {
        let (dir, manifest) = create_test_vault();
        let wiki = dir.path().join("wiki");
        let backend = MockBackend;

        build_index(&manifest, &wiki, None, &backend).await.unwrap();

        // Topic index should exist
        assert!(wiki.join("Rust/_index.md").exists());
        // Top-level index should exist
        assert!(wiki.join("_index.md").exists());
    }

    #[tokio::test]
    async fn topic_index_uses_template_for_small_count() {
        let (dir, manifest) = create_test_vault();
        let wiki = dir.path().join("wiki");
        let backend = MockBackend;

        build_index(&manifest, &wiki, None, &backend).await.unwrap();

        let content = std::fs::read_to_string(wiki.join("Rust/_index.md")).unwrap();
        // Should be template (<=5 notes), contains wikilinks
        assert!(content.contains("# Rust"));
        assert!(content.contains("[["));
    }

    #[tokio::test]
    async fn empty_topic_no_crash() {
        let dir = TempDir::new().unwrap();
        let wiki = dir.path().join("wiki");
        std::fs::create_dir_all(&wiki).unwrap();

        let manifest = Manifest::new();
        let backend = MockBackend;

        // Should not crash with empty manifest
        build_index(&manifest, &wiki, None, &backend).await.unwrap();
        // Top-level index should still be created
        assert!(wiki.join("_index.md").exists());
    }

    #[tokio::test]
    async fn rebuild_after_adding_note() {
        let (dir, mut manifest) = create_test_vault();
        let wiki = dir.path().join("wiki");
        let backend = MockBackend;

        // Build initial index
        build_index(&manifest, &wiki, None, &backend).await.unwrap();
        let initial_content = std::fs::read_to_string(wiki.join("Rust/_index.md")).unwrap();

        // Add a 3rd note
        std::fs::write(wiki.join("Rust/note3_wiki.md"), "# Note 3\nNew content").unwrap();
        manifest.files.insert("note3.md".to_string(), FileEntry {
            sha256: "c".to_string(),
            status: FileStatus::Compiled,
            first_seen: Utc::now(),
            last_processed: Some(Utc::now()),
            output_files: vec!["Rust/note3_wiki.md".to_string()],
            compile_count: 1,
        });
        manifest.topics.get_mut("Rust").unwrap().note_count = 3;

        // Rebuild
        build_index(&manifest, &wiki, None, &backend).await.unwrap();
        let updated_content = std::fs::read_to_string(wiki.join("Rust/_index.md")).unwrap();

        // Index should be different (now includes note3)
        assert_ne!(initial_content, updated_content);
        assert!(updated_content.contains("note3"));
    }
}
