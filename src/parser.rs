use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashMap;

pub fn extract_topic(content: &str) -> Option<String> {
    // Look for topic in frontmatter: topic: xxx
    if let Some(fm) = parse_frontmatter(content) {
        if let Some(topic) = fm.get("topic") {
            return Some(topic.clone());
        }
    }
    // Look for <!-- topic: xxx --> comment
    let re = Regex::new(r"<!--\s*topic:\s*(.+?)\s*-->").unwrap();
    re.captures(content).map(|c| c[1].to_string())
}

pub fn parse_frontmatter(content: &str) -> Option<HashMap<String, String>> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }

    let rest = &trimmed[3..];
    let end = rest.find("\n---")?;
    let fm_block = &rest[..end];

    let mut map = HashMap::new();
    for line in fm_block.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            map.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    Some(map)
}

pub fn parse_json_response(content: &str) -> Result<serde_json::Value> {
    // Try to extract JSON from markdown code block first
    let re = Regex::new(r"```(?:json)?\s*\n([\s\S]*?)\n```").unwrap();
    if let Some(caps) = re.captures(content) {
        return serde_json::from_str(&caps[1]).context("parsing JSON from code block");
    }
    // Try parsing the whole content as JSON
    serde_json::from_str(content).context("parsing JSON response")
}

pub fn extract_tags(content: &str) -> Vec<String> {
    let re = Regex::new(r"#([a-zA-Z0-9_/\-]+)").unwrap();
    re.captures_iter(content)
        .map(|c| c[1].to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_topic_from_frontmatter() {
        let content = "---\ntopic: Rust\ntags: programming\n---\n# Content";
        assert_eq!(extract_topic(content), Some("Rust".to_string()));
    }

    #[test]
    fn extract_topic_from_comment() {
        let content = "# Note\n<!-- topic: Machine Learning -->\nContent here";
        assert_eq!(extract_topic(content), Some("Machine Learning".to_string()));
    }

    #[test]
    fn extract_topic_none() {
        assert_eq!(extract_topic("# No topic here"), None);
    }

    #[test]
    fn parse_frontmatter_basic() {
        let content = "---\ntitle: Test\ntopic: Rust\n---\n# Body";
        let fm = parse_frontmatter(content).unwrap();
        assert_eq!(fm["title"], "Test");
        assert_eq!(fm["topic"], "Rust");
    }

    #[test]
    fn parse_frontmatter_missing() {
        assert!(parse_frontmatter("# No frontmatter").is_none());
    }

    #[test]
    fn parse_json_from_code_block() {
        let content = "Here is the result:\n```json\n{\"score\": 4}\n```\n";
        let val = parse_json_response(content).unwrap();
        assert_eq!(val["score"], 4);
    }

    #[test]
    fn parse_json_direct() {
        let content = r#"{"score": 5, "note": "good"}"#;
        let val = parse_json_response(content).unwrap();
        assert_eq!(val["score"], 5);
    }

    #[test]
    fn extract_tags_basic() {
        let content = "# Note\n#rust #programming/general #ai_ml";
        let tags = extract_tags(content);
        assert_eq!(tags, vec!["rust", "programming/general", "ai_ml"]);
    }

    #[test]
    fn extract_tags_from_heading() {
        // Markdown headings should not match (## has space)
        let content = "## Heading\n#tag1";
        let tags = extract_tags(content);
        assert!(tags.contains(&"tag1".to_string()));
        // "# Heading" — the # before H has space after, regex won't match "Heading" as tag
    }
}
