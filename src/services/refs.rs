use std::path::{Path, PathBuf};

use regex::Regex;

use crate::domain::EdgeType;

#[derive(Clone, Debug)]
pub struct ReferenceHit {
    pub raw: String,
    pub resolved_path: PathBuf,
    pub edge_type: EdgeType,
    pub broken: bool,
}

pub fn extract_references(base_file: &Path, content: &str, home_dir: &Path) -> Vec<ReferenceHit> {
    let markdown = Regex::new(r#"\[[^\]]+\]\(([^)]+)\)"#).expect("markdown regex compiles");
    let import = Regex::new(r#"@([~./][^\s"'`]+)"#).expect("import regex compiles");
    let quoted = Regex::new(r#"["']((?:\.\.?/|~/)[^"'`]+)["']"#).expect("quoted regex compiles");

    let mut hits = Vec::new();
    for capture in markdown.captures_iter(content) {
        if let Some(raw) = capture.get(1) {
            if let Some(hit) = resolve_hit(base_file, raw.as_str(), home_dir, EdgeType::References)
            {
                hits.push(hit);
            }
        }
    }
    for capture in import.captures_iter(content) {
        if let Some(raw) = capture.get(1) {
            if let Some(hit) = resolve_hit(base_file, raw.as_str(), home_dir, EdgeType::Imports) {
                hits.push(hit);
            }
        }
    }
    for capture in quoted.captures_iter(content) {
        if let Some(raw) = capture.get(1) {
            if let Some(hit) = resolve_hit(base_file, raw.as_str(), home_dir, EdgeType::References)
            {
                hits.push(hit);
            }
        }
    }
    hits
}

fn resolve_hit(
    base_file: &Path,
    raw: &str,
    home_dir: &Path,
    edge_type: EdgeType,
) -> Option<ReferenceHit> {
    if raw.starts_with("http://") || raw.starts_with("https://") || raw.starts_with('#') {
        return None;
    }

    let resolved_path = if let Some(stripped) = raw.strip_prefix("~/") {
        home_dir.join(stripped)
    } else {
        base_file.parent().unwrap_or(base_file).join(raw)
    };
    let normalized = resolved_path.components().collect::<PathBuf>();

    Some(ReferenceHit {
        raw: raw.to_string(),
        broken: !normalized.exists(),
        resolved_path: normalized,
        edge_type,
    })
}

#[cfg(test)]
mod tests {
    use super::extract_references;
    use crate::domain::EdgeType;

    #[test]
    fn extracts_markdown_and_imports() {
        let base = std::path::Path::new("/tmp/repo/AGENTS.md");
        let content = r#"
        [doc](./docs/guide.md)
        @./nested/policy.md
        const x = "../other/file.txt";
        "#;
        let refs = extract_references(base, content, std::path::Path::new("/Users/test"));
        assert_eq!(refs.len(), 3);
        assert!(refs.iter().any(|hit| hit.edge_type == EdgeType::Imports));
    }
}
