use std::{fs, net::IpAddr, path::PathBuf, time::Duration};

use anyhow::{anyhow, Result};
use chrono::Utc;
use reqwest::{Client, Url};
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    config::AppConfig,
    domain::{RemoteSnapshot, SnapshotAssociation},
    storage::Store,
};

pub async fn fetch_snapshot(
    config: &AppConfig,
    store: &Store,
    url: &str,
    project_id: Option<&str>,
    tool: Option<&str>,
) -> Result<(RemoteSnapshot, Option<SnapshotAssociation>)> {
    // Existing validation (implementation not shown)
    validate_snapshot_url(config, url)?;

    // Additional SSRF hardening: parse and validate the URL before requesting it.
    let parsed_url = Url::parse(url).map_err(|e| anyhow!("invalid URL: {e}"))?;

    // Only allow HTTP(S) schemes.
    match parsed_url.scheme() {
        "http" | "https" => {}
        _ => {
            return Err(anyhow!("unsupported URL scheme"));
        }
    }

    // Basic host validation to prevent SSRF to local/internal services.
    if let Some(host) = parsed_url.host_str() {
        // Disallow obvious local hostnames.
        let host_lower = host.to_ascii_lowercase();
        if host_lower == "localhost" || host_lower.ends_with(".localhost") {
            return Err(anyhow!("refusing to fetch from localhost"));
        }

        // If the host is a literal IP address, reject private/loopback/link-local ranges.
        if let Ok(ip) = host.parse::<IpAddr>() {
            if is_private_ip(ip) {
                return Err(anyhow!("refusing to fetch from private or local IP address"));
            }
        }
    } else {
        return Err(anyhow!("URL must include a host"));
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;
    let response = client.get(parsed_url).send().await?;
    if let Some(length) = response.content_length() {
        if length > config.max_snapshot_bytes as u64 {
            return Err(anyhow!("snapshot too large"));
        }
    }
    let body = response.text().await?;
    if body.len() > config.max_snapshot_bytes {
        return Err(anyhow!("snapshot too large"));
    }

    let snapshot_id = Uuid::new_v4().to_string();
    let dir = store.snapshot_dir(&snapshot_id);
    fs::create_dir_all(&dir)?;
    let content_path = dir.join("content.html");
    fs::write(&content_path, &body)?;

    let mut hasher = Sha256::new();
    hasher.update(body.as_bytes());
    let normalized_hash = format!("{:x}", hasher.finalize());

    let document = Html::parse_document(&body);
    let selector = Selector::parse("a").expect("anchor selector compiles");
    let linked_urls = document
        .select(&selector)
        .filter_map(|node| node.value().attr("href"))
        .filter(|href| href.starts_with("http://") || href.starts_with("https://"))
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    let snapshot = RemoteSnapshot {
        id: snapshot_id.clone(),
        url: url.to_string(),
        fetched_at: Utc::now(),
        content_path: content_path.to_string_lossy().to_string(),
        normalized_hash,
        linked_urls,
    };

    store.write_json(&dir.join("meta.json"), &snapshot)?;

    let association = match (project_id, tool) {
        (Some(project_id), Some(tool)) => {
            let association = SnapshotAssociation {
                project_id: project_id.to_string(),
                tool: tool.to_string(),
                snapshot: snapshot.clone(),
            };
            let path: PathBuf = store
                .project_dir(project_id)
                .join(format!("remote-snapshot-{tool}.json"));
            store.write_json(&path, &association)?;
            Some(association)
        }
        _ => None,
    };

    Ok((snapshot, association))
}

// Helper to determine whether an IP address is in a private, loopback, or link-local range.
fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.octets()[0] == 169 && v4.octets()[1] == 254
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                // Unique local addresses (fc00::/7)
                || (v6.segments()[0] & 0xfe00) == 0xfc00
        }
    }
}

fn validate_snapshot_url(config: &AppConfig, raw_url: &str) -> Result<()> {
    let url = Url::parse(raw_url)?;
    match url.scheme() {
        "https" => {}
        "http" if config.allow_insecure_doc_hosts => {}
        "http" => {
            return Err(anyhow!(
                "http URLs disabled; use https or set HARNESS_ALLOW_INSECURE_DOC_HOSTS=true"
            ))
        }
        _ => return Err(anyhow!("unsupported URL scheme")),
    }

    let host = url.host_str().ok_or_else(|| anyhow!("missing URL host"))?;
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".local") {
        return Err(anyhow!("local hosts are blocked for docs fetch"));
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        let blocked = match ip {
            IpAddr::V4(v4) => {
                v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_multicast()
            }
            IpAddr::V6(v6) => v6.is_loopback() || v6.is_multicast() || v6.is_unspecified(),
        };
        if blocked {
            return Err(anyhow!(
                "private or loopback addresses are blocked for docs fetch"
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::config::AppConfig;

    use super::validate_snapshot_url;

    fn test_config() -> AppConfig {
        AppConfig {
            home_dir: PathBuf::from("/tmp/home"),
            store_root: PathBuf::from("/tmp/store"),
            default_roots: vec![PathBuf::from("/tmp/home/git")],
            scan_max_depth: 5,
            known_global_dirs: Vec::new(),
            allowed_origins: vec!["http://127.0.0.1:4173".to_string()],
            allow_insecure_doc_hosts: false,
            max_snapshot_bytes: 5_000_000,
        }
    }

    #[test]
    fn blocks_private_hosts() {
        let config = test_config();
        assert!(validate_snapshot_url(&config, "https://127.0.0.1/docs").is_err());
        assert!(validate_snapshot_url(&config, "https://localhost/docs").is_err());
        assert!(validate_snapshot_url(&config, "https://192.168.1.10/docs").is_err());
    }

    #[test]
    fn allows_https_public_hosts() {
        let config = test_config();
        assert!(
            validate_snapshot_url(&config, "https://developers.openai.com/codex/skills").is_ok()
        );
    }
}
