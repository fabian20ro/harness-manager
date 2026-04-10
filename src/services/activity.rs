use std::fs;
use std::path::Path;

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
    let processes = collect_processes()?;
    let evidence = filter_processes(&processes, &catalog.observed_probes, project_root, &catalog.surface);

    let path = store.activity_path(project_id, &catalog.surface);
    store.write_json(&path, &evidence)?;

    let _ = config;
    Ok(evidence)
}

#[cfg(target_os = "linux")]
fn collect_processes() -> Result<Vec<(i32, String)>> {
    let mut processes = Vec::new();
    let entries = match fs::read_dir("/proc") {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()),
    };

    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let file_name = entry.file_name();
        let Some(s) = file_name.to_str() else {
            continue;
        };
        let Ok(pid) = s.parse::<i32>() else {
            continue;
        };

        let cmdline_path = entry.path().join("cmdline");
        // Don't error out if a process disappears while we're reading it
        if let Ok(content) = fs::read(cmdline_path) {
            if content.is_empty() {
                continue;
            }
            // /proc/[pid]/cmdline is null-separated
            let cmdline = content
                .split(|&b| b == 0)
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ");

            if !cmdline.is_empty() {
                processes.push((pid, cmdline));
            }
        }
    }

    Ok(processes)
}

#[cfg(not(target_os = "linux"))]
fn collect_processes() -> Result<Vec<(i32, String)>> {
    use std::process::Command;

    let output = Command::new("ps").args(["-axo", "pid=,command="]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut processes = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, |c: char| c.is_whitespace()).collect();
        if parts.len() == 2 {
            if let Ok(pid) = parts[0].parse::<i32>() {
                processes.push((pid, parts[1].to_string()));
            }
        }
    }

    Ok(processes)
}

fn filter_processes(
    processes: &[(i32, String)],
    probes: &[String],
    project_root: &Path,
    entity_id: &str,
) -> Vec<ObservationEvidence> {
    let mut evidence = Vec::new();
    let project_root_lower = project_root.to_string_lossy().to_ascii_lowercase();

    for probe in probes {
        let Some(token) = probe.strip_prefix("ps:") else {
            continue;
        };
        let token_lower = token.to_ascii_lowercase();

        for (pid, cmdline) in processes {
            let cmdline_lower = cmdline.to_ascii_lowercase();
            // Security: Only capture processes that match the tool token AND are within the project root.
            // This prevents leaking information about unrelated system processes.
            if cmdline_lower.contains(&token_lower) && cmdline_lower.contains(&project_root_lower) {
                evidence.push(ObservationEvidence {
                    entity_id: entity_id.to_string(),
                    source_type: "process".to_string(),
                    captured_at: Utc::now(),
                    payload_ref: format!("{} {}", pid, cmdline.trim()),
                    confidence: 0.95,
                });
            }
        }
    }
    evidence
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_filter_processes() {
        let project_root = PathBuf::from("/home/user/project");
        let entity_id = "test_tool";
        let probes = vec!["ps:claude".to_string(), "ignore:me".to_string()];

        let processes = vec![
            (101, "claude-code --root /home/user/project".to_string()),
            (102, "claude-code --root /home/other/project".to_string()), // Should be ignored (cross-project)
            (103, "other-tool --root /home/user/project".to_string()),    // Should be ignored (no token)
            (104, "CLAUDE-CODE /home/user/PROJECT".to_string()),         // Case insensitivity test
        ];

        let result = filter_processes(&processes, &probes, &project_root, entity_id);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].payload_ref, "101 claude-code --root /home/user/project");
        assert_eq!(result[0].confidence, 0.95);
        assert_eq!(result[1].payload_ref, "104 CLAUDE-CODE /home/user/PROJECT");
    }

    #[test]
    fn test_filter_processes_no_match() {
        let project_root = PathBuf::from("/home/user/project");
        let entity_id = "test_tool";
        let probes = vec!["ps:claude".to_string()];

        let processes = vec![
            (101, "some system process".to_string()),
        ];

        let result = filter_processes(&processes, &probes, &project_root, entity_id);
        assert!(result.is_empty());
    }
}
