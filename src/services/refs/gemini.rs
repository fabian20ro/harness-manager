use regex::Regex;
use crate::domain::EdgeType;
use super::{ReferenceHit, ReferenceSourceKind, ResolverContext};
use super::util::resolve_hit;

pub fn gemini_resolver(context: &ResolverContext<'_>, content: &str) -> Vec<ReferenceHit> {
    let mut hits = Vec::new();
    if context.tool_family != "gemini" {
        return hits;
    }

    let skill_regex = Regex::new(r#"<activated_skill>\s*([^<]+)\s*</activated_skill>"#).expect("skill regex compiles");
    let agent_regex = Regex::new(r#"\b(cli_help|codebase_investigator|generalist)\b"#).expect("agent regex compiles");

    for capture in skill_regex.captures_iter(content) {
        if let Some(skill_name) = capture.get(1).map(|m| m.as_str().trim()) {
            let raw_path = format!("~/.gemini/plugins/skills/{}.md", skill_name);
            if let Some(hit) = resolve_hit(
                context,
                &raw_path,
                EdgeType::Activates,
                ReferenceSourceKind::GenericText,
                "gemini_skill",
                format!("Activated Gemini skill {} in {}.", skill_name, context.base_display_path),
                1.0,
                true,
            ) {
                hits.push(hit);
            }
        }
    }

    for capture in agent_regex.captures_iter(content) {
        if let Some(agent_name) = capture.get(1).map(|m| m.as_str().trim()) {
            let raw_path = format!("~/.gemini/plugins/agents/{}.md", agent_name);
            if let Some(hit) = resolve_hit(
                context,
                &raw_path,
                EdgeType::Activates,
                ReferenceSourceKind::GenericText,
                "gemini_agent",
                format!("Delegated to Gemini agent {} in {}.", agent_name, context.base_display_path),
                0.9,
                true,
            ) {
                hits.push(hit);
            }
        }
    }

    hits
}
