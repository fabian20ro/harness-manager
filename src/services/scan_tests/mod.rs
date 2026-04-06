pub mod discovery;
pub mod indexing;
pub mod references;
pub mod plugins;

#[cfg(test)]
pub(crate) mod common {
    use std::path::Path;
    use chrono::Utc;
    use anyhow::Result;
    use crate::domain::{ProjectKind, ProjectSummary, SurfaceState, ToolCatalog};
    use crate::config::AppConfig;
    use crate::storage::Store;
    use crate::services::graph::{build_surface_state_with_context, ScanRunContext};
    use crate::services::projects::discovery::display_path;

    pub fn build_surface_state(
        config: &AppConfig,
        store: &Store,
        project: &ProjectSummary,
        catalog: &ToolCatalog,
        inventory: &[String],
    ) -> Result<SurfaceState> {
        let mut scan_run = ScanRunContext::default();
        build_surface_state_with_context(
            config,
            store,
            project,
            catalog,
            inventory,
            &mut scan_run,
            &mut |_| Ok(()),
        )
    }

    pub fn demo_project_summary(root: &Path, home: &Path) -> ProjectSummary {
        ProjectSummary {
            id: "demo".to_string(),
            root_path: root.to_string_lossy().to_string(),
            display_path: display_path(root, home),
            name: "demo".to_string(),
            kind: ProjectKind::GitRepo,
            discovery_reason: String::new(),
            signal_score: 300,
            indexed_at: Utc::now(),
            status: "ready".to_string(),
        }
    }
}
