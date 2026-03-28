use std::{fs, path::PathBuf};

use anyhow::Result;
use chrono::Utc;
use reqwest::Client;
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    domain::{RemoteSnapshot, SnapshotAssociation},
    storage::Store,
};

pub async fn fetch_snapshot(
    store: &Store,
    url: &str,
    project_id: Option<&str>,
    tool: Option<&str>,
) -> Result<(RemoteSnapshot, Option<SnapshotAssociation>)> {
    let client = Client::builder().build()?;
    let response = client.get(url).send().await?;
    let body = response.text().await?;

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
