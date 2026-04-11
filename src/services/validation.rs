use std::path::Path;
use std::env;
use std::fs;

use crate::domain::{ArtifactNode, HealthReport, HealthStatus, CheckResult, ValidationRule};

pub fn validate_artifact(artifact: &ArtifactNode, rules: &[ValidationRule]) -> Option<HealthReport> {
    let mut checks = Vec::new();
    let mut overall_status = HealthStatus::Healthy;

    // Standard Rules from Catalog
    for rule in rules {
        let artifact_filename = Path::new(&artifact.path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        if rule.target != artifact_filename && rule.target != format!("{:?}", artifact.artifact_type) {
            continue;
        }

        let result = match rule.rule_type.as_str() {
            "env_var_presence" => check_env_var(&rule.target_env_var().unwrap_or(rule.target.clone())),
            "file_schema" => check_file_schema(&artifact.path, &rule.description),
            _ => None,
        };

        if let Some(res) = result {
            update_overall_status(&mut overall_status, &res.status);
            checks.push(res);
        }
    }

    // Smart Validation: Token Budget
    if let Some(res) = check_token_budget(artifact) {
        update_overall_status(&mut overall_status, &res.status);
        checks.push(res);
    }

    // Smart Validation: Secret Leak Detection (Only for ignore files)
    let filename = Path::new(&artifact.path).file_name().and_then(|n| n.to_str()).unwrap_or("");
    if filename == ".geminiignore" || filename == ".gitignore" {
        if let Some(res) = check_secret_leaks(artifact) {
            update_overall_status(&mut overall_status, &res.status);
            checks.push(res);
        }
    }

    if checks.is_empty() {
        None
    } else {
        Some(HealthReport {
            overall_status,
            checks,
        })
    }
}

fn update_overall_status(overall: &mut HealthStatus, current: &HealthStatus) {
    if *current == HealthStatus::Critical {
        *overall = HealthStatus::Critical;
    } else if *current == HealthStatus::Warning && *overall != HealthStatus::Critical {
        *overall = HealthStatus::Warning;
    } else if *current == HealthStatus::Healthy && *overall == HealthStatus::Unknown {
        *overall = HealthStatus::Healthy;
    }
}

fn check_token_budget(artifact: &ArtifactNode) -> Option<CheckResult> {
    // Heuristic: 1 token approx 4 characters
    let estimated_tokens = artifact.byte_size / 4;
    
    let (status, message) = if estimated_tokens > 50_000 {
        (HealthStatus::Critical, format!("Artifact is extremely heavy (~{} tokens). High risk of context overflow.", estimated_tokens))
    } else if estimated_tokens > 10_000 {
        (HealthStatus::Warning, format!("Artifact is quite large (~{} tokens). Consider optimizing or splitting.", estimated_tokens))
    } else {
        (HealthStatus::Healthy, format!("Context weight is healthy (~{} tokens).", estimated_tokens))
    };

    Some(CheckResult {
        label: "Token Budget".to_string(),
        status,
        message,
        fix_available: false,
    })
}

fn check_secret_leaks(artifact: &ArtifactNode) -> Option<CheckResult> {
    let path = Path::new(&artifact.path);
    let parent = path.parent()?;
    
    // Common secret patterns
    let secret_patterns = [".env", ".key", ".pem", "id_rsa", "credentials"];
    let mut found_secrets = Vec::new();

    if let Ok(entries) = fs::read_dir(parent) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if secret_patterns.iter().any(|p| name.contains(p)) {
                // Ignore the ignore file itself
                if !name.contains("ignore") {
                    found_secrets.push(name);
                }
            }
        }
    }

    if found_secrets.is_empty() {
        return None;
    }

    // Check if these secrets are mentioned in the ignore file
    let content = fs::read_to_string(path).unwrap_or_default();
    let unignored: Vec<String> = found_secrets
        .into_iter()
        .filter(|s| !content.contains(s))
        .collect();

    if unignored.is_empty() {
        Some(CheckResult {
            label: "Secret Protection".to_string(),
            status: HealthStatus::Healthy,
            message: "Project secrets are correctly listed in ignore file.".to_string(),
            fix_available: false,
        })
    } else {
        Some(CheckResult {
            label: "Secret Protection".to_string(),
            status: HealthStatus::Critical,
            message: format!("UNPROTECTED SECRETS: {}. Add to {} immediately.", unignored.join(", "), path.file_name()?.to_string_lossy()),
            fix_available: true,
        })
    }
}

fn check_env_var(var_name: &str) -> Option<CheckResult> {
    let key_exists = env::var(var_name).is_ok();
    
    // 2026 Heuristic: Check if the user is likely authenticated via a Subscription/Editor Session
    // We look for common session marker files or global auth tokens.
    let has_editor_session = check_for_active_editor_session(var_name);

    if key_exists || has_editor_session {
        Some(CheckResult {
            label: format!("Auth: {}", var_name.replace("_API_KEY", "")),
            status: HealthStatus::Healthy,
            message: if key_exists { 
                format!("Authenticated via {} environment variable.", var_name) 
            } else { 
                "Authenticated via active Editor Subscription session.".to_string()
            },
            fix_available: false,
        })
    } else {
        Some(CheckResult {
            label: format!("Auth: {}", var_name.replace("_API_KEY", "")),
            status: HealthStatus::Critical,
            message: format!("No API Key found and no active editor session detected. {} is required.", var_name),
            fix_available: true,
        })
    }
}

fn check_for_active_editor_session(var_name: &str) -> bool {
    let home = env::var("HOME").map(std::path::PathBuf::from).unwrap_or_default();
    
    match var_name {
        "GEMINI_API_KEY" => {
            // Check for Google Cloud or Gemini CLI global auth markers
            home.join(".config/gcloud/access_token").exists() || 
            home.join(".gemini/session.json").exists()
        },
        "ANTHROPIC_API_KEY" => {
            home.join(".claude/session.json").exists()
        },
        "GITHUB_TOKEN" => {
            // Check for gh cli or Copilot extension tokens
            home.join(".config/gh/hosts.yml").exists()
        },
        _ => false
    }
}

fn check_file_schema(path: &str, description: &str) -> Option<CheckResult> {
    let path = Path::new(path);
    if !path.exists() {
        return None;
    }

    let content = fs::read_to_string(path).unwrap_or_default();
    
    // Simple heuristic for 2026 mandates: check for "Mandates" or "Workflow" headers
    let has_mandates = content.contains("# Core Mandates") || content.contains("## Mandates");
    
    Some(CheckResult {
        label: "Schema Validation".to_string(),
        status: if has_mandates { HealthStatus::Healthy } else { HealthStatus::Warning },
        message: if has_mandates {
            "File follows standard mandate structure.".to_string()
        } else {
            format!("Heuristic check failed: {}. Missing mandate sections.", description)
        },
        fix_available: true,
    })
}

pub fn apply_fix(artifact: &ArtifactNode, check_label: &str) -> anyhow::Result<()> {
    match check_label {
        "Schema Validation" => {
            let filename = Path::new(&artifact.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            
            let template_path = format!("src/templates/{}", filename);
            let template_content = if Path::new(&template_path).exists() {
                fs::read_to_string(template_path)?
            } else {
                return Err(anyhow::anyhow!("No template found for {}", filename));
            };

            fs::write(&artifact.path, template_content)?;
            Ok(())
        }
        "Secret Protection" => {
            let path = Path::new(&artifact.path);
            let parent = path.parent().ok_or_else(|| anyhow::anyhow!("No parent dir"))?;
            
            // Re-run the detection to get the missing secrets
            let secret_patterns = [".env", ".key", ".pem", "id_rsa", "credentials"];
            let mut found_secrets = Vec::new();
            if let Ok(entries) = fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if secret_patterns.iter().any(|p| name.contains(p)) && !name.contains("ignore") {
                        found_secrets.push(name);
                    }
                }
            }

            let mut content = fs::read_to_string(path).unwrap_or_default();
            let mut added = false;
            for secret in found_secrets {
                if !content.contains(&secret) {
                    if !content.is_empty() && !content.ends_with('\n') {
                        content.push('\n');
                    }
                    content.push_str(&format!("{}\n", secret));
                    added = true;
                }
            }

            if added {
                fs::write(path, content)?;
            }
            Ok(())
        }
        _ => Err(anyhow::anyhow!("No automated fix for: {}", check_label)),
    }
}

impl ValidationRule {
    fn target_env_var(&self) -> Option<String> {
        if self.rule_type == "env_var_presence" {
            Some(self.target.clone())
        } else {
            None
        }
    }
}
