use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{Result};
use walkdir::WalkDir;

use crate::{
    domain::{
        ProjectDiscoveryRootStrategy, ProjectDiscoveryRule, ProjectKind, ToolCatalog,
    },
};

#[derive(Clone, Debug)]
pub struct ProjectCandidate {
    pub root_path: PathBuf,
    pub name: String,
    pub kind: ProjectKind,
    pub discovery_reason: String,
    pub signal_score: i32,
}

#[derive(Clone, Debug)]
pub struct CandidateSignal {
    pub root_path: PathBuf,
    pub kind: ProjectKind,
    pub score: i32,
    pub reason: String,
}

#[derive(Clone, Debug)]
pub struct CompiledProjectDiscoveryRule {
    pub kind: ProjectKind,
    pub score: i32,
    pub reason: String,
    pub root_strategy: ProjectDiscoveryRootStrategy,
    pub skip_if_scan_root: bool,
    pub matcher: globset::GlobMatcher,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanProgress {
    pub phase: String,
    pub message: String,
    pub current_path: Option<String>,
    pub items_done: Option<usize>,
    pub items_total: Option<usize>,
}

pub fn discover_project_candidates_with_progress(
    roots: &[PathBuf],
    known_global_dirs: &[PathBuf],
    max_depth: usize,
    catalogs: &HashMap<String, ToolCatalog>,
    home_dir: &Path,
    on_progress: &mut dyn FnMut(ScanProgress) -> Result<()>,
) -> Result<Vec<ProjectCandidate>> {
    let mut search_roots = roots.to_vec();
    search_roots.extend(known_global_dirs.iter().cloned());
    search_roots.sort();
    search_roots.dedup();

    let compiled_rules = compile_project_discovery_rules(catalogs)?;

    let mut signals = Vec::new();
    for root in &search_roots {
        if !root.exists() {
            continue;
        }

        on_progress(ScanProgress {
            phase: "root".to_string(),
            message: format!("Scanning root {}", display_path(root, home_dir)),
            current_path: Some(root.to_string_lossy().to_string()),
            items_done: None,
            items_total: None,
        })?;

        if root.join(".git").exists() {
            signals.push(CandidateSignal {
                root_path: root.clone(),
                kind: ProjectKind::GitRepo,
                score: 300,
                reason: "Directory contains .git.".to_string(),
            });
        }

        for entry in WalkDir::new(root)
            .max_depth(max_depth)
            .into_iter()
            .filter_entry(|entry| should_traverse_candidate_entry(entry))
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if entry.file_type().is_dir() && path.join(".git").exists() {
                signals.push(CandidateSignal {
                    root_path: path.to_path_buf(),
                    kind: ProjectKind::GitRepo,
                    score: 300,
                    reason: "Directory contains .git.".to_string(),
                });
            }

            let relative = path.strip_prefix(root).unwrap_or(path);
            for rule in &compiled_rules {
                if project_discovery_rule_matches(relative, root, &rule.matcher) {
                    if let Some(project_root) = resolve_project_discovery_root(path, root, &rule.root_strategy) {
                        if rule.skip_if_scan_root && !project_root.starts_with(root) {
                            continue;
                        }
                        signals.push(CandidateSignal {
                            root_path: project_root,
                            kind: rule.kind.clone(),
                            score: rule.score,
                            reason: rule.reason.clone(),
                        });
                    }
                }
            }
        }
    }

    Ok(finalize_project_candidates(signals))
}

fn compile_project_discovery_rules(
    catalogs: &HashMap<String, ToolCatalog>,
) -> Result<Vec<CompiledProjectDiscoveryRule>> {
    let mut compiled = Vec::new();
    for catalog in catalogs.values() {
        for ProjectDiscoveryRule {
            glob,
            kind,
            score,
            reason,
            root_strategy,
            skip_if_scan_root,
        } in &catalog.project_discovery_rules
        {
            compiled.push(CompiledProjectDiscoveryRule {
                kind: kind.clone(),
                score: *score,
                reason: reason.clone(),
                root_strategy: root_strategy.clone(),
                skip_if_scan_root: *skip_if_scan_root,
                matcher: globset::Glob::new(glob)?.compile_matcher(),
            });
        }
    }
    Ok(compiled)
}

fn project_discovery_rule_matches(
    relative: &Path,
    scan_root: &Path,
    matcher: &globset::GlobMatcher,
) -> bool {
    project_discovery_match_candidates(relative, scan_root)
        .into_iter()
        .any(|candidate| matcher.is_match(candidate))
}

fn project_discovery_match_candidates(relative: &Path, scan_root: &Path) -> Vec<String> {
    let components = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    if components.is_empty() {
        return Vec::new();
    }

    let mut matches = Vec::new();
    for index in 0..components.len() {
        matches.push(components[index..].join("/"));
    }

    if let Some(root_name) = scan_root.file_name().and_then(|name| name.to_str()) {
        if !root_name.is_empty() {
            let prefixed = matches
                .iter()
                .map(|candidate| format!("{root_name}/{candidate}"))
                .collect::<Vec<_>>();
            matches.extend(prefixed);
        }
    }

    matches.sort();
    matches.dedup();
    matches
}

fn resolve_project_discovery_root(
    path: &Path,
    scan_root: &Path,
    strategy: &ProjectDiscoveryRootStrategy,
) -> Option<PathBuf> {
    match strategy {
        ProjectDiscoveryRootStrategy::MatchParent => path.parent().map(Path::to_path_buf),
        ProjectDiscoveryRootStrategy::LevelsUp { count } => {
            let mut current = if is_hidden_plugin_manifest(path) {
                path.parent()?
            } else {
                path
            };
            for _ in 0..*count {
                current = current.parent()?;
            }
            Some(current.to_path_buf())
        }
        ProjectDiscoveryRootStrategy::NearestPluginRoot => {
            Some(nearest_plugin_root(path.parent()?, scan_root))
        }
    }
}

fn is_hidden_plugin_manifest(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some("plugin.json")
        && matches!(
            path.parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str()),
            Some(".codex-plugin" | ".claude-plugin")
        )
}

fn nearest_plugin_root(start: &Path, scan_root: &Path) -> PathBuf {
    let mut current = Some(start);
    while let Some(path) = current {
        if path.join(".codex-plugin").join("plugin.json").exists()
            || path.join(".claude-plugin").join("plugin.json").exists()
            || path.join(".mcp.json").exists()
        {
            return path.to_path_buf();
        }
        if path.file_name().and_then(|name| name.to_str()) == Some("skills") {
            return path.parent().unwrap_or(path).to_path_buf();
        }
        if path == scan_root {
            break;
        }
        current = path.parent();
    }
    start.to_path_buf()
}

fn finalize_project_candidates(signals: Vec<CandidateSignal>) -> Vec<ProjectCandidate> {
    let mut merged = HashMap::<(PathBuf, ProjectKind), ProjectCandidate>::new();
    for signal in signals {
        let key = (signal.root_path.clone(), signal.kind.clone());
        let name = signal
            .root_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string();
        merged
            .entry(key)
            .and_modify(|candidate| {
                let prior_score = candidate.signal_score;
                candidate.signal_score = candidate.signal_score.max(signal.score);
                if candidate.discovery_reason.is_empty() || signal.score > prior_score {
                    candidate.discovery_reason = signal.reason.clone();
                }
            })
            .or_insert(ProjectCandidate {
                root_path: signal.root_path,
                name,
                kind: signal.kind,
                discovery_reason: signal.reason,
                signal_score: signal.score,
            });
    }

    let mut candidates = merged.into_values().collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        project_kind_rank(&left.kind)
            .cmp(&project_kind_rank(&right.kind))
            .then(right.signal_score.cmp(&left.signal_score))
            .then(left.root_path.cmp(&right.root_path))
    });

    let mut kept = Vec::<ProjectCandidate>::new();
    'candidate: for candidate in candidates {
        for existing in &kept {
            if candidate.root_path == existing.root_path {
                if project_kind_rank(&candidate.kind) >= project_kind_rank(&existing.kind) {
                    continue 'candidate;
                }
            }
            if candidate.root_path.starts_with(&existing.root_path) {
                if matches!(existing.kind, ProjectKind::GitRepo) {
                    continue 'candidate;
                }
                if candidate.kind == existing.kind {
                    continue 'candidate;
                }
            }
        }
        kept.push(candidate);
    }

    kept.sort_by(|left, right| {
        project_kind_rank(&left.kind)
            .cmp(&project_kind_rank(&right.kind))
            .then(right.signal_score.cmp(&left.signal_score))
            .then(left.name.cmp(&right.name))
            .then(left.root_path.cmp(&right.root_path))
    });
    kept
}

fn project_kind_rank(kind: &ProjectKind) -> i32 {
    match kind {
        ProjectKind::GitRepo => 0,
        ProjectKind::WorkspaceCandidate => 1,
        ProjectKind::PluginPackage => 2,
    }
}

pub fn display_path(path: &Path, home_dir: &Path) -> String {
    if let Ok(relative) = path.strip_prefix(home_dir) {
        let relative = relative.to_string_lossy();
        if relative.is_empty() {
            "~".to_string()
        } else {
            format!("~/{}", relative)
        }
    } else {
        path.to_string_lossy().to_string()
    }
}

fn should_traverse_candidate_entry(entry: &walkdir::DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy();
    !matches!(name.as_ref(), ".git" | "node_modules" | "target" | "dist")
}
