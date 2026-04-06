use std::{
    fs,
    path::Path,
};

use serde_json::{Map as JsonMap, Value as JsonValue};

#[derive(Clone, Debug, Default)]
pub struct ParsedSkillMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
    pub metadata: Option<JsonValue>,
}

pub fn parse_skill_metadata(skill_path: &Path) -> ParsedSkillMetadata {
    let Ok(content) = fs::read_to_string(skill_path) else {
        return ParsedSkillMetadata::default();
    };
    let Some(frontmatter) = extract_frontmatter(&content) else {
        return attach_openai_metadata(skill_path, ParsedSkillMetadata::default());
    };
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&frontmatter) else {
        return attach_openai_metadata(skill_path, ParsedSkillMetadata::default());
    };

    let mut metadata = ParsedSkillMetadata {
        name: yaml_field_as_string(&value, "name"),
        description: yaml_field_as_string(&value, "description"),
        metadata: None,
    };

    let legacy = legacy_frontmatter_metadata(&value);
    metadata = attach_openai_metadata(skill_path, metadata);
    if let Some(legacy_value) = legacy {
        merge_skill_metadata(&mut metadata, "legacy_frontmatter", legacy_value);
    }
    metadata
}

fn extract_frontmatter(content: &str) -> Option<String> {
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut frontmatter = Vec::new();
    for line in lines {
        if line.trim() == "---" {
            return Some(frontmatter.join("\n"));
        }
        frontmatter.push(line);
    }
    None
}

fn yaml_field_as_string(value: &serde_yaml::Value, key: &str) -> Option<String> {
    value
        .as_mapping()
        .and_then(|mapping| mapping.get(&serde_yaml::Value::String(key.to_string())))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn legacy_frontmatter_metadata(value: &serde_yaml::Value) -> Option<JsonValue> {
    let mapping = value.as_mapping()?;
    let mut legacy = JsonMap::new();
    for key in [
        "retrieval",
        "intents",
        "entities",
        "pathPatterns",
        "bashPatterns",
    ] {
        let Some(entry) = mapping.get(&serde_yaml::Value::String(key.to_string())) else {
            continue;
        };
        let Ok(json_value) = serde_json::to_value(entry) else {
            continue;
        };
        legacy.insert(key.to_string(), json_value);
    }
    (!legacy.is_empty()).then(|| JsonValue::Object(legacy))
}

fn attach_openai_metadata(skill_path: &Path, mut metadata: ParsedSkillMetadata) -> ParsedSkillMetadata {
    let skill_dir = skill_path.parent().unwrap_or(skill_path);
    for candidate in [
        skill_dir.join("agents").join("openai.yaml"),
        skill_dir.join("agents").join("openai.yml"),
    ] {
        let Ok(content) = fs::read_to_string(&candidate) else {
            continue;
        };
        let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&content) else {
            continue;
        };
        if let Ok(json_value) = serde_json::to_value(value) {
            merge_skill_metadata(&mut metadata, "openai", json_value);
            break;
        }
    }
    metadata
}

fn merge_skill_metadata(metadata: &mut ParsedSkillMetadata, key: &str, value: JsonValue) {
    let mut root = metadata
        .metadata
        .take()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    root.insert(key.to_string(), value);
    metadata.metadata = Some(JsonValue::Object(root));
}
