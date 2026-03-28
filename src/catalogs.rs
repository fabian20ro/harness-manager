use std::{collections::HashMap, path::PathBuf};

use anyhow::{Context, Result};

use crate::domain::ToolCatalog;

const CATALOG_FILES: &[(&str, &str)] = &[
    ("codex", include_str!("../catalogs/seed/codex.json")),
    ("codex_cli", include_str!("../catalogs/seed/codex_cli.json")),
    (
        "claude_code",
        include_str!("../catalogs/seed/claude_code.json"),
    ),
    (
        "claude_cowork",
        include_str!("../catalogs/seed/claude_cowork.json"),
    ),
    (
        "copilot_cli",
        include_str!("../catalogs/seed/copilot_cli.json"),
    ),
    (
        "intellij_copilot",
        include_str!("../catalogs/seed/intellij_copilot.json"),
    ),
    ("opencode", include_str!("../catalogs/seed/opencode.json")),
    (
        "antigravity",
        include_str!("../catalogs/seed/antigravity.json"),
    ),
];

pub fn seed_catalogs() -> Result<Vec<ToolCatalog>> {
    CATALOG_FILES
        .iter()
        .map(|(_, raw)| serde_json::from_str(raw).context("invalid seed catalog"))
        .collect()
}

pub fn seed_catalog_map() -> Result<HashMap<String, ToolCatalog>> {
    let catalogs = seed_catalogs()?;
    Ok(catalogs
        .into_iter()
        .map(|catalog| (catalog.surface.clone(), catalog))
        .collect())
}

pub fn catalog_path(root: &PathBuf, surface: &str, version: &str) -> PathBuf {
    root.join("catalogs")
        .join(surface)
        .join(format!("{version}.json"))
}
