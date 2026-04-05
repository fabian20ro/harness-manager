use regex::Regex;
use crate::domain::EdgeType;
use super::{ReferenceHit, ReferenceSourceKind, ResolverContext};
use super::util::resolve_hit;

pub fn generic_text_resolver(context: &ResolverContext<'_>, content: &str) -> Vec<ReferenceHit> {
    let markdown = Regex::new(r#"\[[^\]]+\]\(([^)]+)\)"#).expect("markdown regex compiles");
    let import = Regex::new(r#"@([~./][^\s"'`]+)"#).expect("import regex compiles");
    let quoted = Regex::new(r#"["']((?:\.\.?/|~/)[^"'`]+)["']"#).expect("quoted regex compiles");

    let mut hits = Vec::new();
    for capture in markdown.captures_iter(content) {
        if let Some(raw) = capture.get(1).map(|matched| matched.as_str()) {
            if let Some(hit) = resolve_hit(
                context,
                raw,
                EdgeType::References,
                ReferenceSourceKind::GenericText,
                "markdown_reference",
                format!("Markdown reference found in {}.", context.base_display_path),
                0.72,
                false,
            ) {
                hits.push(hit);
            }
        }
    }
    for capture in import.captures_iter(content) {
        if let Some(raw) = capture.get(1).map(|matched| matched.as_str()) {
            if let Some(hit) = resolve_hit(
                context,
                raw,
                EdgeType::Imports,
                ReferenceSourceKind::GenericText,
                "generic_import",
                format!(
                    "Import-like reference found in {}.",
                    context.base_display_path
                ),
                0.74,
                false,
            ) {
                hits.push(hit);
            }
        }
    }
    for capture in quoted.captures_iter(content) {
        if let Some(raw) = capture.get(1).map(|matched| matched.as_str()) {
            if let Some(hit) = resolve_hit(
                context,
                raw,
                EdgeType::References,
                ReferenceSourceKind::GenericText,
                "quoted_reference",
                format!(
                    "Quoted path-like reference found in {}.",
                    context.base_display_path
                ),
                0.68,
                false,
            ) {
                hits.push(hit);
            }
        }
    }
    hits
}
