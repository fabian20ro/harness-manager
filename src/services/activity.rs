use std::{path::Path, process::Command};

use anyhow::Result;
use chrono::Utc;

use crate::{
    config::AppConfig,
    domain::{ObservationEvidence, ToolCatalog},
    storage::Store,
};

pub fn refresh_activity(
    config: &AppConfig,
    store: &Store,
    project_id: &str,
    project_root: &Path,
    catalog: &ToolCatalog,
) -> Result<Vec<ObservationEvidence>> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,command="])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut evidence = Vec::new();

    for probe in &catalog.observed_probes {
        let Some(token) = probe.strip_prefix("ps:") else {
            continue;
        };
        for line in stdout.lines() {
            let lower = line.to_ascii_lowercase();
            if lower.contains(token)
                && (lower.contains(&project_root.to_string_lossy().to_ascii_lowercase())
                    || lower.contains(token))
            {
                evidence.push(ObservationEvidence {
                    entity_id: catalog.surface.clone(),
                    source_type: "process".to_string(),
                    captured_at: Utc::now(),
                    payload_ref: line.trim().to_string(),
                    confidence: if lower
                        .contains(&project_root.to_string_lossy().to_ascii_lowercase())
                    {
                        0.95
                    } else {
                        0.6
                    },
                });
            }
        }
    }

    let path = store.activity_path(project_id, &catalog.surface);
    store.write_json(&path, &evidence)?;

    let _ = config;
    Ok(evidence)
}
