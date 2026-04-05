use std::path::{Path, PathBuf};
use regex::Regex;
use serde_json::{json, Value as JsonValue};

pub mod config;
pub mod gemini;
pub mod generic;
pub mod instructions;
pub mod util;

use crate::domain::{ArtifactType, EdgeType};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReferenceSourceKind {
    GenericText,
    InstructionImport,
    InstructionDirective,
    InstructionTableRef,
    InstructionCodeSpanRef,
    TypedConfigField,
    PluginManifest,
}

#[derive(Clone, Debug)]
pub struct ResolverContext<'a> {
    pub base_file: &'a Path,
    pub resolve_from_dir: &'a Path,
    pub base_display_path: &'a str,
    pub artifact_type: &'a ArtifactType,
    pub tool_family: &'a str,
    pub home_dir: &'a Path,
}

#[derive(Clone, Debug)]
pub struct ReferenceHit {
    pub raw: String,
    pub resolved_path: PathBuf,
    pub edge_type: EdgeType,
    pub broken: bool,
    pub source_kind: ReferenceSourceKind,
    pub source: String,
    pub reason: String,
    pub confidence: f32,
    pub promotes_effective: bool,
}

pub fn extract_metadata(context: &ResolverContext<'_>, content: &str) -> Option<JsonValue> {
    if context.tool_family != "gemini" {
        return None;
    }

    let mut metadata = serde_json::Map::new();

    let contexts = [
        ("global_context", r#"<global_context>\s*(.*?)\s*</global_context>"#),
        ("extension_context", r#"<extension_context>\s*(.*?)\s*</extension_context>"#),
        ("project_context", r#"<project_context>\s*(.*?)\s*</project_context>"#),
    ];

    let mut context_objects = Vec::new();
    for (tag, pattern) in contexts.iter() {
        if let Ok(regex) = Regex::new(&format!("(?s){}", pattern)) {
            for capture in regex.captures_iter(content) {
                if let Some(match_content) = capture.get(1).map(|m| m.as_str().trim().to_string()) {
                    context_objects.push(json!({
                        "type": tag,
                        "content": match_content
                    }));
                }
            }
        }
    }

    if !context_objects.is_empty() {
        metadata.insert("contexts".to_string(), json!(context_objects));
    }

    if let Ok(memory_regex) = Regex::new(r#"(?s)## Gemini Added Memories\s*\n(.*?)(?:\n## |$)"#) {
        if let Some(capture) = memory_regex.captures(content) {
            if let Some(memory_content) = capture.get(1).map(|m| m.as_str().trim().to_string()) {
                metadata.insert("global_memories".to_string(), json!(memory_content));
            }
        }
    }

    if context.base_file.file_name().and_then(|n| n.to_str()) == Some(".geminiignore") {
        let gitignore_path = context.base_file.with_file_name(".gitignore");
        if let Ok(gitignore_content) = std::fs::read_to_string(&gitignore_path) {
            metadata.insert("adjacent_gitignore".to_string(), json!(gitignore_content));
        }
    }

    if metadata.is_empty() {
        None
    } else {
        Some(JsonValue::Object(metadata))
    }
}

pub fn extract_references(context: &ResolverContext<'_>, content: &str) -> Vec<ReferenceHit> {
    let mut hits = Vec::new();
    hits.extend(instructions::base_file_resolver(context, content));
    hits.extend(config::typed_config_resolver(context, content));
    hits.extend(generic::generic_text_resolver(context, content));
    hits.extend(gemini::gemini_resolver(context, content));
    util::dedupe_hits(hits)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};
    use tempfile::TempDir;

    use super::{extract_references, ReferenceSourceKind, ResolverContext};
    use crate::domain::{ArtifactType, EdgeType};

    #[test]
    fn extracts_instruction_imports_with_high_confidence() {
        let temp = TempDir::new().expect("tempdir");
        let base = temp.path().join("AGENTS.md");
        let target = temp.path().join("policy.md");
        fs::write(&target, "ok").expect("target");
        let content = "@./policy.md\n";

        let refs = extract_references(
            &ResolverContext {
                base_file: &base,
                resolve_from_dir: temp.path(),
                base_display_path: "AGENTS.md",
                artifact_type: &ArtifactType::Instructions,
                tool_family: "codex",
                home_dir: Path::new("/Users/test"),
            },
            content,
        );

        assert!(refs.iter().any(|hit| hit.edge_type == EdgeType::Imports));
        assert!(refs.iter().any(|hit| hit.source == "instruction_import"));
        assert!(refs.iter().any(|hit| hit.confidence > 0.9));
        assert!(refs.iter().any(|hit| hit.promotes_effective));
    }

    #[test]
    fn extracts_instruction_directives_with_bare_filenames() {
        let temp = TempDir::new().expect("tempdir");
        let base = temp.path().join("AGENTS.md");
        let target = temp.path().join("CLAUDE.md");
        fs::write(&target, "ok").expect("target");

        let refs = extract_references(
            &ResolverContext {
                base_file: &base,
                resolve_from_dir: temp.path(),
                base_display_path: "AGENTS.md",
                artifact_type: &ArtifactType::Instructions,
                tool_family: "codex",
                home_dir: Path::new("/Users/test"),
            },
            "Read CLAUDE.md\n",
        );

        assert!(refs.iter().any(|hit| hit.source == "instruction_directive"));
        assert!(refs
            .iter()
            .any(|hit| hit.source_kind == ReferenceSourceKind::InstructionDirective));
        assert!(refs
            .iter()
            .any(|hit| hit.resolved_path.ends_with("CLAUDE.md") && hit.promotes_effective));
    }

    #[test]
    fn extracts_multiple_instruction_directives_from_sentence() {
        let temp = TempDir::new().expect("tempdir");
        let base = temp.path().join("CLAUDE.md");
        fs::write(temp.path().join("ANALYSIS.md"), "ok").expect("analysis");
        fs::write(temp.path().join("TODOS.md"), "ok").expect("todos");

        let refs = extract_references(
            &ResolverContext {
                base_file: &base,
                resolve_from_dir: temp.path(),
                base_display_path: "CLAUDE.md",
                artifact_type: &ArtifactType::Instructions,
                tool_family: "claude",
                home_dir: Path::new("/Users/test"),
            },
            "If prioritization is involved, read `ANALYSIS.md` and `TODOS.md` directly before planning.\n",
        );

        assert!(refs
            .iter()
            .any(|hit| hit.resolved_path.ends_with("ANALYSIS.md")));
        assert!(refs
            .iter()
            .any(|hit| hit.resolved_path.ends_with("TODOS.md")));
    }

    #[test]
    fn extracts_instruction_table_refs_from_docs_map() {
        let temp = TempDir::new().expect("tempdir");
        let base = temp.path().join("CLAUDE.md");
        fs::create_dir_all(temp.path().join("docs").join("CODEMAPS")).expect("docs dir");
        fs::write(temp.path().join("docs").join("CONTRIB.md"), "ok").expect("contrib");
        fs::write(
            temp.path().join("docs").join("CODEMAPS").join("architecture.md"),
            "ok",
        )
        .expect("architecture");

        let refs = extract_references(
            &ResolverContext {
                base_file: &base,
                resolve_from_dir: temp.path(),
                base_display_path: "CLAUDE.md",
                artifact_type: &ArtifactType::Instructions,
                tool_family: "claude",
                home_dir: Path::new("/Users/test"),
            },
            "| Need | Read |\n|---|---|\n| Conventions | `docs/CONTRIB.md` |\n| Architecture | `docs/CODEMAPS/architecture.md` |\n",
        );

        assert!(refs
            .iter()
            .any(|hit| hit.source_kind == ReferenceSourceKind::InstructionTableRef));
        assert!(refs
            .iter()
            .any(|hit| hit.resolved_path.ends_with("docs/CONTRIB.md")));
        assert!(refs
            .iter()
            .any(|hit| hit.resolved_path.ends_with("docs/CODEMAPS/architecture.md")));
    }

    #[test]
    fn extracts_typed_toml_paths() {
        let temp = TempDir::new().expect("tempdir");
        let base = temp.path().join("config.toml");
        let included = temp.path().join("rules").join("policy.md");
        fs::create_dir_all(included.parent().expect("parent")).expect("mkdir");
        fs::write(&included, "ok").expect("policy");

        let refs = extract_references(
            &ResolverContext {
                base_file: &base,
                resolve_from_dir: temp.path(),
                base_display_path: "config.toml",
                artifact_type: &ArtifactType::Config,
                tool_family: "codex",
                home_dir: Path::new("/Users/test"),
            },
            r#"
            [instructions]
            include = "./rules/policy.md"
            "#,
        );

        assert!(refs.iter().any(|hit| hit.source == "typed_config"));
        assert!(refs
            .iter()
            .any(|hit| hit.resolved_path.ends_with("rules/policy.md")));
        assert!(refs.iter().any(|hit| hit.promotes_effective));
    }

    #[test]
    fn falls_back_to_generic_reference_detection() {
        let base = Path::new("/tmp/repo/README.md");
        let refs = extract_references(
            &ResolverContext {
                base_file: base,
                resolve_from_dir: base.parent().expect("parent"),
                base_display_path: "README.md",
                artifact_type: &ArtifactType::LocalDoc,
                tool_family: "misc",
                home_dir: Path::new("/Users/test"),
            },
            r#"See "./other.md""#,
        );
        assert!(refs.iter().any(|hit| hit.source == "quoted_reference"));
        assert!(refs.iter().any(|hit| !hit.promotes_effective));
    }

    #[test]
    fn plugin_manifest_refs_resolve_from_plugin_root_for_codex_layouts() {
        let temp = TempDir::new().expect("tempdir");
        let plugin_root = temp.path().join("vercel");
        let manifest_dir = plugin_root.join(".codex-plugin");
        let manifest = manifest_dir.join("plugin.json");
        fs::create_dir_all(plugin_root.join("skills")).expect("skills dir");
        fs::create_dir_all(&manifest_dir).expect("manifest dir");
        fs::write(plugin_root.join("skills").join("skill.md"), "ok").expect("skill");

        let refs = extract_references(
            &ResolverContext {
                base_file: &manifest,
                resolve_from_dir: &plugin_root,
                base_display_path: "~/.codex/.tmp/plugins/plugins/vercel/.codex-plugin/plugin.json",
                artifact_type: &ArtifactType::PluginManifest,
                tool_family: "codex",
                home_dir: Path::new("/Users/test"),
            },
            r#"{ "skills": "./skills/skill.md" }"#,
        );

        assert!(refs
            .iter()
            .any(|hit| hit.resolved_path == plugin_root.join("skills").join("skill.md")));
    }

    #[test]
    fn plugin_manifest_refs_resolve_from_plugin_root_for_claude_layouts() {
        let temp = TempDir::new().expect("tempdir");
        let plugin_root = temp.path().join("everything-claude-code");
        let manifest_dir = plugin_root.join(".claude-plugin");
        let manifest = manifest_dir.join("plugin.json");
        fs::create_dir_all(plugin_root.join("agents")).expect("agents dir");
        fs::create_dir_all(&manifest_dir).expect("manifest dir");
        fs::write(plugin_root.join("agents").join("architect.md"), "ok").expect("agent");

        let refs = extract_references(
            &ResolverContext {
                base_file: &manifest,
                resolve_from_dir: &plugin_root,
                base_display_path: "~/.claude/plugins/marketplaces/everything-claude-code/.claude-plugin/plugin.json",
                artifact_type: &ArtifactType::PluginManifest,
                tool_family: "claude",
                home_dir: Path::new("/Users/test"),
            },
            r#"{ "agents": ["./agents/architect.md"] }"#,
        );

        assert!(refs
            .iter()
            .any(|hit| hit.resolved_path == plugin_root.join("agents").join("architect.md")));
    }

    #[test]
    fn extracts_gemini_metadata_contexts_and_memories() {
        let temp = tempfile::TempDir::new().expect("tempdir");
        let base = temp.path().join("GEMINI.md");
        std::fs::write(
            &base,
            r#"
<global_context>
System wide rule
</global_context>

<project_context>
Project specific rule
</project_context>

## Gemini Added Memories
I prefer using tabs
## Other Section
"#,
        ).expect("file");

        let context = super::ResolverContext {
            base_file: &base,
            resolve_from_dir: temp.path(),
            base_display_path: "GEMINI.md",
            artifact_type: &super::ArtifactType::Instructions,
            tool_family: "gemini",
            home_dir: Path::new("/Users/test"),
        };

        let content = std::fs::read_to_string(&base).unwrap();
        let meta = super::extract_metadata(&context, &content).expect("metadata");
        let obj = meta.as_object().unwrap();

        let contexts = obj.get("contexts").unwrap().as_array().unwrap();
        assert_eq!(contexts.len(), 2);
        assert_eq!(contexts[0]["type"], "global_context");
        assert_eq!(contexts[0]["content"], "System wide rule");

        assert_eq!(obj.get("global_memories").unwrap().as_str().unwrap(), "I prefer using tabs");
    }
}
