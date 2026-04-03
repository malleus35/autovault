use std::path::Path;

const COMPILE_NOTE: &str = include_str!("../prompts/compile_note.md");
const COMPILE_MERGE: &str = include_str!("../prompts/compile_merge.md");
const INDEX_TOPIC: &str = include_str!("../prompts/index_topic.md");
const INDEX_TOP: &str = include_str!("../prompts/index_top.md");
const LINT_CHECK: &str = include_str!("../prompts/lint_check.md");
const QA_REVIEW: &str = include_str!("../prompts/qa_review.md");
const CONFLICT_CHECK: &str = include_str!("../prompts/conflict_check.md");

pub fn get_prompt(name: &str, vault_prompts_dir: Option<&Path>) -> Option<String> {
    // Check for vault override first
    if let Some(dir) = vault_prompts_dir {
        let override_path = dir.join(format!("{}.md", name));
        if override_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&override_path) {
                return Some(content);
            }
        }
    }

    // Fall back to built-in
    match name {
        "compile_note" => Some(COMPILE_NOTE.to_string()),
        "compile_merge" => Some(COMPILE_MERGE.to_string()),
        "index_topic" => Some(INDEX_TOPIC.to_string()),
        "index_top" => Some(INDEX_TOP.to_string()),
        "lint_check" => Some(LINT_CHECK.to_string()),
        "qa_review" => Some(QA_REVIEW.to_string()),
        "conflict_check" => Some(CONFLICT_CHECK.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn builtin_prompts_exist() {
        for name in [
            "compile_note", "compile_merge", "index_topic", "index_top",
            "lint_check", "qa_review", "conflict_check",
        ] {
            assert!(
                get_prompt(name, None).is_some(),
                "missing built-in prompt: {}",
                name
            );
        }
    }

    #[test]
    fn unknown_prompt_returns_none() {
        assert!(get_prompt("nonexistent", None).is_none());
    }

    #[test]
    fn vault_override_takes_precedence() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("compile_note.md"), "CUSTOM PROMPT").unwrap();

        let result = get_prompt("compile_note", Some(dir.path())).unwrap();
        assert_eq!(result, "CUSTOM PROMPT");
    }

    #[test]
    fn falls_back_when_no_override() {
        let dir = TempDir::new().unwrap();
        // no override file
        let result = get_prompt("compile_note", Some(dir.path())).unwrap();
        assert!(!result.is_empty());
        assert_ne!(result, "CUSTOM PROMPT");
    }
}
