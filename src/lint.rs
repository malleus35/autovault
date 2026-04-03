use anyhow::Result;
use std::path::Path;

use crate::llm::LlmBackend;
use crate::parser::parse_frontmatter;
use crate::prompts::get_prompt;
use crate::utils::atomic_write;

#[derive(Debug, Clone)]
pub struct LintIssue {
    pub file: String,
    pub rule: String,
    pub message: String,
    pub fixable: bool,
}

pub struct LintResult {
    pub issues: Vec<LintIssue>,
    pub fixed: usize,
}

pub async fn lint(
    wiki_dir: &Path,
    prompts_dir: Option<&Path>,
    backend: Option<&dyn LlmBackend>,
    deep: bool,
    fix: bool,
) -> Result<LintResult> {
    let mut issues = Vec::new();
    let mut fixed = 0;

    // Scan all wiki .md files
    let wiki_files = collect_md_files(wiki_dir)?;

    for path in &wiki_files {
        let filename = path
            .strip_prefix(wiki_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();
        let content = std::fs::read_to_string(path)?;

        // Rule 1: Missing frontmatter
        if parse_frontmatter(&content).is_none() && !filename.contains("_index") {
            let issue = LintIssue {
                file: filename.clone(),
                rule: "missing-frontmatter".to_string(),
                message: "File has no YAML frontmatter".to_string(),
                fixable: true,
            };
            if fix {
                let fixed_content = format!("---\ntopic: unknown\n---\n{}", content);
                atomic_write(path, fixed_content.as_bytes())?;
                fixed += 1;
            }
            issues.push(issue);
        }

        // Rule 2: Empty body
        let body = extract_body(&content);
        if body.trim().is_empty() {
            issues.push(LintIssue {
                file: filename.clone(),
                rule: "empty-body".to_string(),
                message: "File has no content after frontmatter".to_string(),
                fixable: false,
            });
        }

        // Rule 3: Broken wikilinks
        let broken = find_broken_wikilinks(&content, wiki_dir);
        for link in broken {
            issues.push(LintIssue {
                file: filename.clone(),
                rule: "broken-wikilink".to_string(),
                message: format!("Wikilink [[{}]] target not found", link),
                fixable: false,
            });
        }

        // Rule 4: Deep lint (LLM-based)
        if deep {
            if let Some(backend) = backend {
                let prompt = get_prompt("lint_check", prompts_dir)
                    .unwrap_or_else(|| "Review this note for quality issues.".to_string());
                if let Ok(response) = backend.call(&prompt, &content).await {
                    if let Ok(json) = crate::parser::parse_json_response(&response.content) {
                        if let Some(lint_issues) = json.get("issues").and_then(|v| v.as_array()) {
                            for item in lint_issues {
                                if let Some(msg) = item.get("message").and_then(|v| v.as_str()) {
                                    issues.push(LintIssue {
                                        file: filename.clone(),
                                        rule: "semantic".to_string(),
                                        message: msg.to_string(),
                                        fixable: false,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Rule 5: Orphan notes (compiled wiki files not linked from any index)
    let index_files: Vec<&std::path::PathBuf> = wiki_files
        .iter()
        .filter(|p| p.file_name().map(|n| n == "_index.md").unwrap_or(false))
        .collect();

    let mut all_index_content = String::new();
    for idx in &index_files {
        if let Ok(c) = std::fs::read_to_string(idx) {
            all_index_content.push_str(&c);
        }
    }

    for path in &wiki_files {
        let fname = path.file_name().unwrap().to_string_lossy();
        if fname == "_index.md" {
            continue;
        }
        let note_name = fname.replace("_wiki.md", "").replace(".md", "");
        if !all_index_content.contains(&note_name) {
            let filename = path
                .strip_prefix(wiki_dir)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            issues.push(LintIssue {
                file: filename,
                rule: "orphan-note".to_string(),
                message: format!("Note '{}' not referenced in any index", note_name),
                fixable: false,
            });
        }
    }

    Ok(LintResult { issues, fixed })
}

fn extract_body(content: &str) -> &str {
    let trimmed = content.trim_start();
    if let Some(rest) = trimmed.strip_prefix("---") {
        if let Some(pos) = rest.find("\n---") {
            return &rest[pos + 4..];
        }
    }
    content
}

fn find_broken_wikilinks(content: &str, wiki_dir: &Path) -> Vec<String> {
    let re = regex::Regex::new(r"\[\[([^\]|]+)(?:\|[^\]]*)?\]\]").unwrap();
    let mut broken = Vec::new();

    for cap in re.captures_iter(content) {
        let link = cap[1].trim();
        // Check if any file matching link exists
        let possible = wiki_dir.join(format!("{}.md", link));
        let possible_wiki = wiki_dir.join(format!("{}_wiki.md", link));
        let possible_nested = find_file_recursive(wiki_dir, &format!("{}.md", link));
        let possible_nested_wiki = find_file_recursive(wiki_dir, &format!("{}_wiki.md", link));

        if !possible.exists()
            && !possible_wiki.exists()
            && !possible_nested
            && !possible_nested_wiki
        {
            broken.push(link.to_string());
        }
    }

    broken
}

fn find_file_recursive(dir: &Path, filename: &str) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.file_name().map(|n| n == filename).unwrap_or(false) {
                return true;
            }
            if path.is_dir() && find_file_recursive(&path, filename) {
                return true;
            }
        }
    }
    false
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

    struct MockLintBackend;

    #[async_trait]
    impl LlmBackend for MockLintBackend {
        async fn call(&self, _prompt: &str, _input: &str) -> Result<LlmResponse> {
            Ok(LlmResponse {
                content: r#"{"issues": [{"message": "Vague terminology detected"}]}"#.to_string(),
                duration: std::time::Duration::from_millis(10),
                token_count: None,
            })
        }
        fn name(&self) -> &str { "mock_lint" }
    }

    fn setup_wiki() -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let wiki = dir.path().join("wiki");
        let topic = wiki.join("Rust");
        std::fs::create_dir_all(&topic).unwrap();
        (dir, wiki)
    }

    #[tokio::test]
    async fn detect_missing_frontmatter() {
        let (_dir, wiki) = setup_wiki();
        std::fs::write(wiki.join("Rust/note_wiki.md"), "# No frontmatter here").unwrap();

        let result = lint(&wiki, None, None, false, false).await.unwrap();
        assert!(result.issues.iter().any(|i| i.rule == "missing-frontmatter"));
    }

    #[tokio::test]
    async fn detect_empty_body() {
        let (_dir, wiki) = setup_wiki();
        std::fs::write(wiki.join("Rust/note_wiki.md"), "---\ntopic: Rust\n---\n").unwrap();

        let result = lint(&wiki, None, None, false, false).await.unwrap();
        assert!(result.issues.iter().any(|i| i.rule == "empty-body"));
    }

    #[tokio::test]
    async fn detect_broken_wikilink() {
        let (_dir, wiki) = setup_wiki();
        std::fs::write(
            wiki.join("Rust/note_wiki.md"),
            "---\ntopic: Rust\n---\n# Note\nSee [[nonexistent]]",
        ).unwrap();

        let result = lint(&wiki, None, None, false, false).await.unwrap();
        assert!(result.issues.iter().any(|i| i.rule == "broken-wikilink"));
    }

    #[tokio::test]
    async fn no_issues_for_valid_file() {
        let (_dir, wiki) = setup_wiki();
        // Create a valid file and its index
        std::fs::write(
            wiki.join("Rust/note_wiki.md"),
            "---\ntopic: Rust\n---\n# Valid Note\nContent here",
        ).unwrap();
        std::fs::write(
            wiki.join("Rust/_index.md"),
            "# Rust\n- [[note]]",
        ).unwrap();

        let result = lint(&wiki, None, None, false, false).await.unwrap();
        // Should have no structural issues (frontmatter, body, broken links)
        let structural: Vec<_> = result.issues.iter()
            .filter(|i| i.rule != "orphan-note")
            .collect();
        assert!(structural.is_empty(), "Got issues: {:?}", structural);
    }

    #[tokio::test]
    async fn fix_missing_frontmatter() {
        let (_dir, wiki) = setup_wiki();
        let path = wiki.join("Rust/note_wiki.md");
        std::fs::write(&path, "# No frontmatter").unwrap();

        let result = lint(&wiki, None, None, false, true).await.unwrap();
        assert_eq!(result.fixed, 1);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("---\ntopic:"));
    }

    #[tokio::test]
    async fn deep_lint_calls_llm() {
        let (_dir, wiki) = setup_wiki();
        std::fs::write(
            wiki.join("Rust/note_wiki.md"),
            "---\ntopic: Rust\n---\n# Note\nSome content",
        ).unwrap();
        std::fs::write(wiki.join("Rust/_index.md"), "# Rust\n- [[note]]").unwrap();

        let backend = MockLintBackend;
        let result = lint(&wiki, None, Some(&backend as &dyn LlmBackend), true, false).await.unwrap();
        assert!(result.issues.iter().any(|i| i.rule == "semantic"));
    }

    #[tokio::test]
    async fn detect_orphan_note() {
        let (_dir, wiki) = setup_wiki();
        std::fs::write(
            wiki.join("Rust/orphan_wiki.md"),
            "---\ntopic: Rust\n---\n# Orphan\nNot in any index",
        ).unwrap();
        std::fs::write(wiki.join("Rust/_index.md"), "# Rust\n- [[other]]").unwrap();

        let result = lint(&wiki, None, None, false, false).await.unwrap();
        assert!(result.issues.iter().any(|i| i.rule == "orphan-note"));
    }
}
