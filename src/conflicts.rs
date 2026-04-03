use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::llm::LlmBackend;
use crate::parser::extract_tags;
use crate::prompts::get_prompt;
use crate::utils::atomic_write;

#[derive(Serialize, Deserialize, Debug)]
pub struct Conflict {
    pub file_a: String,
    pub file_b: String,
    pub shared_tags: Vec<String>,
    pub severity: String,
    pub explanation: String,
}

pub struct ConflictsResult {
    pub conflicts: Vec<Conflict>,
}

pub async fn detect_conflicts(
    wiki_dir: &Path,
    state_dir: &Path,
    prompts_dir: Option<&Path>,
    backend: &dyn LlmBackend,
) -> Result<ConflictsResult> {
    // Build tag → files map
    let mut tag_map: HashMap<String, Vec<(String, String)>> = HashMap::new(); // tag → [(filename, content)]

    let wiki_files = collect_md_files(wiki_dir)?;
    for path in &wiki_files {
        if path.file_name().map(|n| n == "_index.md").unwrap_or(false) {
            continue;
        }
        let filename = path
            .strip_prefix(wiki_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();
        let content = std::fs::read_to_string(path)?;
        let tags = extract_tags(&content);

        for tag in &tags {
            tag_map
                .entry(tag.clone())
                .or_default()
                .push((filename.clone(), content.clone()));
        }
    }

    // Find pairs with overlapping tags
    let mut checked_pairs: HashSet<(String, String)> = HashSet::new();
    let mut conflicts = Vec::new();

    let prompt = get_prompt("conflict_check", prompts_dir)
        .context("conflict_check prompt not found")?;

    for files in tag_map.values() {
        if files.len() < 2 {
            continue;
        }

        for i in 0..files.len() {
            for j in (i + 1)..files.len() {
                let (ref name_a, ref content_a) = files[i];
                let (ref name_b, ref content_b) = files[j];

                let pair = if name_a < name_b {
                    (name_a.clone(), name_b.clone())
                } else {
                    (name_b.clone(), name_a.clone())
                };

                if checked_pairs.contains(&pair) {
                    continue;
                }
                checked_pairs.insert(pair);

                // Find all shared tags between this pair
                let tags_a: HashSet<String> = extract_tags(content_a).into_iter().collect();
                let tags_b: HashSet<String> = extract_tags(content_b).into_iter().collect();
                let shared: Vec<String> = tags_a.intersection(&tags_b).cloned().collect();

                if shared.is_empty() {
                    continue;
                }

                let input = format!(
                    "## File A: {}\n{}\n\n## File B: {}\n{}\n\nShared tags: {}",
                    name_a,
                    content_a,
                    name_b,
                    content_b,
                    shared.join(", ")
                );

                if let Ok(response) = backend.call(&prompt, &input).await {
                    if let Ok(json) = crate::parser::parse_json_response(&response.content) {
                        let has_conflict = json
                            .get("conflict")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);

                        if has_conflict {
                            conflicts.push(Conflict {
                                file_a: name_a.clone(),
                                file_b: name_b.clone(),
                                shared_tags: shared,
                                severity: json
                                    .get("severity")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown")
                                    .to_string(),
                                explanation: json
                                    .get("explanation")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    // Save to state/conflicts.json
    let json = serde_json::to_string_pretty(&conflicts)?;
    std::fs::create_dir_all(state_dir)?;
    atomic_write(&state_dir.join("conflicts.json"), json.as_bytes())?;

    Ok(ConflictsResult { conflicts })
}

fn collect_md_files(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    if !dir.exists() {
        return Ok(files);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_md_files(&path)?);
        } else if path.extension().map(|e| e == "md").unwrap_or(false) {
            files.push(path);
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{LlmBackend, LlmResponse};
    use async_trait::async_trait;
    use tempfile::TempDir;

    struct ConflictMockBackend {
        has_conflict: bool,
    }

    #[async_trait]
    impl LlmBackend for ConflictMockBackend {
        async fn call(&self, _prompt: &str, _input: &str) -> Result<LlmResponse> {
            let content = if self.has_conflict {
                r#"{"conflict": true, "severity": "high", "explanation": "Contradictory definitions"}"#
            } else {
                r#"{"conflict": false}"#
            };
            Ok(LlmResponse {
                content: content.to_string(),
                duration: std::time::Duration::from_millis(10),
                token_count: None,
            })
        }
        fn name(&self) -> &str { "mock_conflict" }
    }

    #[tokio::test]
    async fn detect_conflict_pair() {
        let dir = TempDir::new().unwrap();
        let wiki = dir.path().join("wiki");
        let state = dir.path().join("state");
        std::fs::create_dir_all(wiki.join("Topic")).unwrap();

        std::fs::write(
            wiki.join("Topic/note1_wiki.md"),
            "---\ntopic: Topic\n---\n# Note 1\n#rust #programming\nRust is systems language",
        ).unwrap();
        std::fs::write(
            wiki.join("Topic/note2_wiki.md"),
            "---\ntopic: Topic\n---\n# Note 2\n#rust #programming\nRust is a scripting language",
        ).unwrap();

        let backend = ConflictMockBackend { has_conflict: true };
        let result = detect_conflicts(&wiki, &state, None, &backend).await.unwrap();

        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.conflicts[0].severity, "high");

        // Verify conflicts.json was written
        assert!(state.join("conflicts.json").exists());
    }

    #[tokio::test]
    async fn no_conflict_pair() {
        let dir = TempDir::new().unwrap();
        let wiki = dir.path().join("wiki");
        let state = dir.path().join("state");
        std::fs::create_dir_all(wiki.join("Topic")).unwrap();

        std::fs::write(
            wiki.join("Topic/note1_wiki.md"),
            "---\ntopic: Topic\n---\n# Note 1\n#rust #programming\nConsistent info",
        ).unwrap();
        std::fs::write(
            wiki.join("Topic/note2_wiki.md"),
            "---\ntopic: Topic\n---\n# Note 2\n#rust #programming\nMore consistent info",
        ).unwrap();

        let backend = ConflictMockBackend { has_conflict: false };
        let result = detect_conflicts(&wiki, &state, None, &backend).await.unwrap();

        assert!(result.conflicts.is_empty());
    }
}
