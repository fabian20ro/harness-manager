use std::{
    collections::HashSet,
    path::PathBuf,
};

use crate::domain::EdgeType;
use super::{ReferenceHit, ReferenceSourceKind, ResolverContext};

pub fn resolve_hit(
    context: &ResolverContext<'_>,
    raw: &str,
    edge_type: EdgeType,
    source_kind: ReferenceSourceKind,
    source: &str,
    reason: String,
    confidence: f32,
    promotes_effective: bool,
) -> Option<ReferenceHit> {
    if raw.starts_with("http://") || raw.starts_with("https://") || raw.starts_with('#') {
        return None;
    }

    let resolved_path = if let Some(stripped) = raw.strip_prefix("~/") {
        context.home_dir.join(stripped)
    } else {
        context.resolve_from_dir.join(raw)
    };
    let normalized = resolved_path.components().collect::<PathBuf>();

    Some(ReferenceHit {
        raw: raw.to_string(),
        broken: !normalized.exists(),
        resolved_path: normalized,
        edge_type,
        source_kind,
        source: source.to_string(),
        reason,
        confidence,
        promotes_effective,
    })
}

pub fn dedupe_hits(hits: Vec<ReferenceHit>) -> Vec<ReferenceHit> {
    let mut seen = HashSet::new();
    let mut output = Vec::new();

    for hit in hits {
        let key = format!(
            "{}::{:?}::{}",
            hit.resolved_path.display(),
            hit.edge_type,
            hit.source
        );
        if seen.insert(key) {
            output.push(hit);
        }
    }

    output.sort_by(|left, right| {
        right
            .confidence
            .partial_cmp(&left.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    output
}
