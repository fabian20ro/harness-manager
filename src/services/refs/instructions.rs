use regex::Regex;
use crate::domain::EdgeType;
use super::{ReferenceHit, ReferenceSourceKind, ResolverContext};
use super::util::resolve_hit;

pub fn base_file_resolver(context: &ResolverContext<'_>, content: &str) -> Vec<ReferenceHit> {
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

pub fn instruction_imports(context: &ResolverContext<'_>, content: &str) -> Vec<ReferenceHit> {
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
    hits.extend(instruction_structured_refs(context, content));
    hits
}

pub fn instruction_directives(context: &ResolverContext<'_>, content: &str) -> Vec<ReferenceHit> {
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
                        ReferenceSourceKind::InstructionDirective,
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

pub fn instruction_structured_refs(context: &ResolverContext<'_>, content: &str) -> Vec<ReferenceHit> {
    let code_span = Regex::new(r"`([^`\n]+\.(?:md|toml|json|yaml|yml|txt))`")
        .expect("instruction code span regex");

    content
        .lines()
        .flat_map(|line| {
            let is_table = line.contains('|');
            code_span
                .captures_iter(line)
                .filter_map(|capture| capture.get(1).map(|matched| matched.as_str()))
                .filter_map(|raw| {
                    resolve_hit(
                        context,
                        raw,
                        EdgeType::References,
                        if is_table {
                            ReferenceSourceKind::InstructionTableRef
                        } else {
                            ReferenceSourceKind::InstructionCodeSpanRef
                        },
                        if is_table {
                            "instruction_table_ref"
                        } else {
                            "instruction_code_span"
                        },
                        if is_table {
                            format!(
                                "Instruction table reference found in {}.",
                                context.base_display_path
                            )
                        } else {
                            format!(
                                "Instruction code-span reference found in {}.",
                                context.base_display_path
                            )
                        },
                        if is_table { 0.95 } else { 0.91 },
                        true,
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}
