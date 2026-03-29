use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use regex::Regex;
use serde_json::Value as JsonValue;

use crate::domain::{ArtifactType, EdgeType};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReferenceSourceKind {
    GenericText,
    InstructionImport,
    TypedConfigField,
    PluginManifest,
}

#[derive(Clone, Debug)]
pub struct ResolverContext<'a> {
    pub base_file: &'a Path,
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

pub fn extract_references(context: &ResolverContext<'_>, content: &str) -> Vec<ReferenceHit> {
    let mut hits = Vec::new();
    hits.extend(base_file_resolver(context, content));
    hits.extend(typed_config_resolver(context, content));
    hits.extend(generic_text_resolver(context, content));
    dedupe_hits(hits)
}

fn base_file_resolver(context: &ResolverContext<'_>, content: &str) -> Vec<ReferenceHit> {
    let file_name = context
        .base_file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    match file_name {
        "AGENTS.md" | "CLAUDE.md" => instruction_imports(context, content),
        _ => Vec::new(),
    }
}

fn instruction_imports(context: &ResolverContext<'_>, content: &str) -> Vec<ReferenceHit> {
    let import = Regex::new(r"(?m)^\s*@([~./][^\s]+)\s*$").expect("instruction import regex");
    let mut hits = import
        .captures_iter(content)
        .filter_map(|capture| capture.get(1).map(|matched| matched.as_str()))
        .filter_map(|raw| {
            resolve_hit(
                context,
                raw,
                EdgeType::Imports,
                ReferenceSourceKind::InstructionImport,
                "instruction_import",
                format!("Instruction import found in {}.", context.base_display_path),
                0.97,
                true,
            )
        })
        .collect::<Vec<_>>();
    hits.extend(instruction_directives(context, content));
    hits
}

fn instruction_directives(context: &ResolverContext<'_>, content: &str) -> Vec<ReferenceHit> {
    let directive_verb =
        Regex::new(r"(?i)\b(read|see|use|follow|load|consult)\b").expect("directive verb regex");
    let file_token =
        Regex::new(r"`?([A-Za-z0-9_./~\-]+\.(?:md|toml|json|yaml|yml|txt))`?")
            .expect("directive file token regex");

    content
        .lines()
        .filter(|line| directive_verb.is_match(line))
        .flat_map(|line| {
            file_token
                .captures_iter(line)
                .filter_map(|capture| capture.get(1).map(|matched| matched.as_str()))
                .filter_map(|raw| {
                    resolve_hit(
                        context,
                        raw,
                        EdgeType::References,
                        ReferenceSourceKind::InstructionImport,
                        "instruction_directive",
                        format!(
                            "Instruction directive found in {}.",
                            context.base_display_path
                        ),
                        0.93,
                        true,
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn typed_config_resolver(context: &ResolverContext<'_>, content: &str) -> Vec<ReferenceHit> {
    let extension = context
        .base_file
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default();

    let typed_values = match extension {
        "toml" => toml::from_str::<toml::Value>(content)
            .ok()
            .map(TypedValue::Toml),
        "json" => serde_json::from_str::<JsonValue>(content)
            .ok()
            .map(TypedValue::Json),
        "yaml" | "yml" => serde_yaml::from_str::<serde_yaml::Value>(content)
            .ok()
            .map(TypedValue::Yaml),
        _ => None,
    };

    typed_values
        .map(|value| collect_typed_paths(context, value))
        .unwrap_or_default()
}

fn generic_text_resolver(context: &ResolverContext<'_>, content: &str) -> Vec<ReferenceHit> {
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

fn resolve_hit(
    context: &ResolverContext<'_>,
    raw: &str,
    edge_type: EdgeType,
    source_kind: ReferenceSourceKind,
    source: &str,
    reason: String,
    confidence: f32,
    promotes_effective: bool,
) -> Option<ReferenceHit> {
    if raw.starts_with("http://") || raw.starts_with("https://") || raw.starts_with('#') {
        return None;
    }

    let resolved_path = if let Some(stripped) = raw.strip_prefix("~/") {
        context.home_dir.join(stripped)
    } else {
        context
            .base_file
            .parent()
            .unwrap_or(context.base_file)
            .join(raw)
    };
    let normalized = resolved_path.components().collect::<PathBuf>();

    Some(ReferenceHit {
        raw: raw.to_string(),
        broken: !normalized.exists(),
        resolved_path: normalized,
        edge_type,
        source_kind,
        source: source.to_string(),
        reason,
        confidence,
        promotes_effective,
    })
}

fn dedupe_hits(hits: Vec<ReferenceHit>) -> Vec<ReferenceHit> {
    let mut seen = HashSet::new();
    let mut output = Vec::new();

    for hit in hits {
        let key = format!(
            "{}::{:?}::{}",
            hit.resolved_path.display(),
            hit.edge_type,
            hit.source
        );
        if seen.insert(key) {
            output.push(hit);
        }
    }

    output.sort_by(|left, right| {
        right
            .confidence
            .partial_cmp(&left.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    output
}

enum TypedValue {
    Toml(toml::Value),
    Json(JsonValue),
    Yaml(serde_yaml::Value),
}

fn collect_typed_paths(context: &ResolverContext<'_>, value: TypedValue) -> Vec<ReferenceHit> {
    let mut hits = Vec::new();
    let mut trail = Vec::new();
    let mut seen = HashSet::new();
    match value {
        TypedValue::Toml(value) => collect_toml(context, &value, &mut trail, &mut seen, &mut hits),
        TypedValue::Json(value) => collect_json(context, &value, &mut trail, &mut seen, &mut hits),
        TypedValue::Yaml(value) => collect_yaml(context, &value, &mut trail, &mut seen, &mut hits),
    }
    hits
}

fn collect_toml(
    context: &ResolverContext<'_>,
    value: &toml::Value,
    trail: &mut Vec<String>,
    seen: &mut HashSet<String>,
    hits: &mut Vec<ReferenceHit>,
) {
    match value {
        toml::Value::String(text) => maybe_push_typed_hit(context, text, trail, seen, hits),
        toml::Value::Array(values) => {
            for value in values {
                collect_toml(context, value, trail, seen, hits);
            }
        }
        toml::Value::Table(map) => {
            for (key, value) in map {
                trail.push(key.clone());
                collect_toml(context, value, trail, seen, hits);
                trail.pop();
            }
        }
        _ => {}
    }
}

fn collect_json(
    context: &ResolverContext<'_>,
    value: &JsonValue,
    trail: &mut Vec<String>,
    seen: &mut HashSet<String>,
    hits: &mut Vec<ReferenceHit>,
) {
    match value {
        JsonValue::String(text) => maybe_push_typed_hit(context, text, trail, seen, hits),
        JsonValue::Array(values) => {
            for value in values {
                collect_json(context, value, trail, seen, hits);
            }
        }
        JsonValue::Object(map) => {
            for (key, value) in map {
                trail.push(key.clone());
                collect_json(context, value, trail, seen, hits);
                trail.pop();
            }
        }
        _ => {}
    }
}

fn collect_yaml(
    context: &ResolverContext<'_>,
    value: &serde_yaml::Value,
    trail: &mut Vec<String>,
    seen: &mut HashSet<String>,
    hits: &mut Vec<ReferenceHit>,
) {
    match value {
        serde_yaml::Value::String(text) => maybe_push_typed_hit(context, text, trail, seen, hits),
        serde_yaml::Value::Sequence(values) => {
            for value in values {
                collect_yaml(context, value, trail, seen, hits);
            }
        }
        serde_yaml::Value::Mapping(map) => {
            for (key, value) in map {
                let key_text = match key {
                    serde_yaml::Value::String(text) => text.clone(),
                    other => format!("{other:?}"),
                };
                trail.push(key_text);
                collect_yaml(context, value, trail, seen, hits);
                trail.pop();
            }
        }
        _ => {}
    }
}

fn maybe_push_typed_hit(
    context: &ResolverContext<'_>,
    text: &str,
    trail: &[String],
    seen: &mut HashSet<String>,
    hits: &mut Vec<ReferenceHit>,
) {
    let path_like = is_path_like(text);
    let key_path = trail.join(".");
    let interesting_key = is_interesting_key(&key_path);
    let allow = match context.artifact_type {
        ArtifactType::PluginManifest | ArtifactType::Config => path_like || interesting_key,
        ArtifactType::Instructions | ArtifactType::LocalDoc => path_like || interesting_key,
        _ => path_like && interesting_key,
    };

    if !allow {
        return;
    }

    let dedupe_key = format!("{key_path}::{text}");
    if !seen.insert(dedupe_key) {
        return;
    }

    if let Some(hit) = resolve_hit(
        context,
        text,
        EdgeType::References,
        if matches!(context.artifact_type, ArtifactType::PluginManifest) {
            ReferenceSourceKind::PluginManifest
        } else {
            ReferenceSourceKind::TypedConfigField
        },
        if matches!(context.artifact_type, ArtifactType::PluginManifest) {
            "plugin_manifest"
        } else {
            "typed_config"
        },
        format!(
            "Typed config reference found in key `{}` of {}.",
            if key_path.is_empty() {
                "<root>"
            } else {
                &key_path
            },
            context.base_display_path
        ),
        0.9,
        true,
    ) {
        hits.push(hit);
    }
}

fn is_path_like(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with("~/")
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || trimmed.starts_with('/')
        || trimmed.ends_with(".md")
        || trimmed.ends_with(".toml")
        || trimmed.ends_with(".json")
        || trimmed.ends_with(".yaml")
        || trimmed.ends_with(".yml")
        || trimmed.ends_with(".txt")
}

fn is_interesting_key(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    let keys = [
        "path",
        "paths",
        "include",
        "includes",
        "import",
        "imports",
        "instructions",
        "readme",
        "docs",
        "doc",
        "file",
        "files",
        "manifest",
        "config",
        "hooks",
        "mcp",
        "skills",
        "agents",
        "rules",
    ];
    let lower = path.to_ascii_lowercase();
    keys.iter().any(|key| {
        lower == *key || lower.ends_with(&format!(".{key}")) || lower.contains(&format!(".{key}."))
    })
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
            .any(|hit| hit.source_kind == ReferenceSourceKind::InstructionImport));
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
    fn extracts_typed_toml_paths() {
        let temp = TempDir::new().expect("tempdir");
        let base = temp.path().join("config.toml");
        let included = temp.path().join("rules").join("policy.md");
        fs::create_dir_all(included.parent().expect("parent")).expect("mkdir");
        fs::write(&included, "ok").expect("policy");

        let refs = extract_references(
            &ResolverContext {
                base_file: &base,
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
}
