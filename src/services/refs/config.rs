use std::collections::HashSet;
use serde_json::Value as JsonValue;
use crate::domain::{ArtifactType, EdgeType};
use super::{ReferenceHit, ReferenceSourceKind, ResolverContext};
use super::util::resolve_hit;

pub fn typed_config_resolver(context: &ResolverContext<'_>, content: &str) -> Vec<ReferenceHit> {
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
